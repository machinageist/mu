// Author: machinageist
// Description: Bounded pager-owned Kitty virtual images, independent of library types.
// Protocol: https://sw.kovidgoyal.net/kitty/graphics-protocol/
// Checked against Ghostty v1.4.0 src/terminal/kitty/graphics_{exec,unicode}.zig.
// The caller draws fallback text first, registers owned_ids() BEFORE writing frame(),
// and calls written() only after a successful write AND flush. Overlays use body-only
// CUP coordinates; the caller must reposition the status bar and end synchronized output.

// Reuse the production encoder in the CLI without exposing library internals.
// The decoder and its sentinel are unused in this encoder-only module instance.
#[allow(dead_code)]
#[path = "../image/base64.rs"]
mod base64;

use std::collections::hash_map::DefaultHasher;
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime};

const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_IMAGES: usize = 8;
const MAX_OWNED_IDS: usize = MAX_IMAGES * 2;
const MAX_PLACEMENTS: usize = 8;
const MAX_SCENE_IMAGES: usize = 4096;
const MAX_DRAW_CELLS: usize = MAX_IMAGES * DIACRITICS.len() * DIACRITICS.len();
const REPLY_WAIT: Duration = Duration::from_millis(1000);
// 3072 raw bytes encode to the protocol's 4096-byte base64 payload limit.
const RAW_CHUNK: usize = 3072;
const APC: &str = "\x1b_G";
const ST: &str = "\x1b\\";
const RESET: &str = "\x1b[0m";
const PLACEHOLDER: char = '\u{10eeee}';

/// A full-resolution, row-major straight RGBA raster and its fallback cell box.
/// Coordinates are zero-based rendered document coordinates, including its gutters.
#[derive(Clone, Debug)]
pub(super) struct Image {
    pub(super) line: usize,
    pub(super) column: usize,
    pub(super) columns: usize,
    pub(super) rows: usize,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) pixels: Arc<[u8]>,
}

/// Body rectangle only: top/left are document offsets, rows excludes status/chrome.
#[derive(Clone, Copy, Debug)]
pub(super) struct Viewport {
    pub(super) top: usize,
    pub(super) left: usize,
    pub(super) rows: usize,
    pub(super) columns: usize,
}

#[derive(Clone)]
struct Raster {
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
    hash: u64,
}

impl Raster {
    fn same(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.hash == other.hash
            && (Arc::ptr_eq(&self.pixels, &other.pixels) || self.pixels == other.pixels)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Grid {
    columns: usize,
    rows: usize,
}

#[derive(Clone, Copy)]
struct SceneImage {
    source: usize,
    line: usize,
    column: usize,
    grid: Grid,
}

#[derive(Clone, Copy)]
struct Clip {
    top: usize,
    bottom: usize,
    left: usize,
    right: usize,
}

impl SceneImage {
    fn clip(self, view: Viewport) -> Option<Clip> {
        let clip = Clip {
            top: self.line.max(view.top),
            bottom: (self.line + self.grid.rows).min(view.top.saturating_add(view.rows)),
            left: self.column.max(view.left),
            right: (self.column + self.grid.columns).min(view.left.saturating_add(view.columns)),
        };
        (clip.top < clip.bottom && clip.left < clip.right).then_some(clip)
    }
}

#[derive(Clone, Copy)]
enum State {
    New,
    Emitted,
    Waiting(Instant),
    Ready,
}

impl State {
    fn deadline(self) -> Option<Instant> {
        match self {
            Self::Waiting(deadline) => Some(deadline),
            Self::New | Self::Emitted | Self::Ready => None,
        }
    }

    fn written(&mut self, now: Instant) {
        if matches!(self, Self::Emitted) {
            *self = Self::Waiting(now + REPLY_WAIT);
        }
    }
}

struct Placement {
    id: u32,
    grid: Grid,
    state: State,
}

struct Cached {
    source: usize,
    raster: Raster,
    id: u32,
    upload: State,
    placements: Vec<Placement>,
    used: u64,
}

struct Retired {
    id: u32,
    emitted: bool,
}

/// At most 32 MiB of distinct retained source pixels; at most eight live terminal
/// assets, eight virtual grids per asset, and sixteen owned IDs including retirement.
/// Rejecting either ACK stage or exceeding its deadline disables native graphics
/// for this manager's lifetime. This deliberately avoids retry/redraw busy loops.
/// Virtual placements do not draw themselves: replacing the previous body's text
/// removes old visible fragments, while the bounded asset cache remains reusable.
pub(super) struct Manager {
    sources: Vec<Raster>,
    scene: Vec<SceneImage>,
    cache: Vec<Cached>,
    retired: Vec<Retired>,
    clock: u64,
    disabled: bool,
}

impl Manager {
    pub(super) fn new(images: Vec<Image>) -> Self {
        let mut manager = Self {
            sources: Vec::new(),
            scene: Vec::new(),
            cache: Vec::new(),
            retired: Vec::new(),
            clock: 0,
            disabled: false,
        };
        manager.reset(images);
        manager
    }

    /// Reuse equal assets/grids after reflow; obsolete grids retire their whole owned
    /// image (including data). Deletion is queued, not written, and remains owned
    /// until a subsequent frame/cleanup has been successfully flushed and written().
    pub(super) fn reset(&mut self, images: Vec<Image>) {
        self.sources.clear();
        self.scene.clear();
        if self.disabled {
            return;
        }
        let mut bytes = 0;
        for image in images.into_iter().take(MAX_SCENE_IMAGES) {
            let Some(size) = pixel_bytes(&image) else {
                continue;
            };
            let grid = Grid {
                columns: image.columns,
                rows: image.rows,
            };
            if grid.columns == 0
                || grid.rows == 0
                || grid.columns > DIACRITICS.len()
                || grid.rows > DIACRITICS.len()
                || image.line.checked_add(grid.rows).is_none()
                || image.column.checked_add(grid.columns).is_none()
            {
                continue;
            }
            let source = self.sources.iter().position(|raster| {
                raster.width == image.width
                    && raster.height == image.height
                    && Arc::ptr_eq(&raster.pixels, &image.pixels)
            });
            let source = if let Some(source) = source {
                source
            } else {
                let mut hasher = DefaultHasher::new();
                image.pixels.hash(&mut hasher);
                let raster = Raster {
                    width: image.width,
                    height: image.height,
                    pixels: image.pixels,
                    hash: hasher.finish(),
                };
                if let Some(source) = self.sources.iter().position(|old| old.same(&raster)) {
                    source
                } else {
                    if size > MAX_BYTES - bytes {
                        continue;
                    }
                    bytes += size;
                    self.sources.push(raster);
                    self.sources.len() - 1
                }
            };
            self.scene.push(SceneImage {
                source,
                line: image.line,
                column: image.column,
                grid,
            });
        }
        let mut index = 0;
        while index < self.cache.len() {
            let cached = &self.cache[index];
            let source = self
                .sources
                .iter()
                .position(|raster| raster.same(&cached.raster));
            if let Some(source) = source
                && cached.placements.iter().all(|placement| {
                    self.scene
                        .iter()
                        .any(|image| image.source == source && image.grid == placement.grid)
                })
            {
                self.cache[index].source = source;
                // Rebind to the new Arc, so equal-but-reallocated source pixels
                // don't double the retained memory across a reflow.
                self.cache[index].raster = self.sources[source].clone();
                index += 1;
            } else {
                self.retire(index);
            }
        }
    }

    /// Generate controls and explicitly addressed placeholder fragments only.
    /// Call owned_ids() AFTER this call, but BEFORE any bytes reach the terminal.
    /// show=false suppresses new transmissions/placements/cells, not expiry/deletion.
    pub(super) fn frame(&mut self, view: Viewport, show: bool, now: Instant) -> String {
        self.expire(now);
        let mut output = String::new();
        if !show || self.disabled || view.rows == 0 || view.columns == 0 {
            self.deletions(&mut output);
            return output;
        }
        self.clock = self.clock.saturating_add(1);
        let visible: Vec<_> = self
            .scene
            .iter()
            .filter_map(|&image| image.clip(view).map(|clip| (image, clip)))
            .collect();
        // A stable first-eight admission set prevents >8 visible assets from
        // evicting/re-uploading each other on every redraw.
        let mut wanted = Vec::new();
        for (image, _) in &visible {
            if !wanted.contains(&image.source) && wanted.len() < MAX_IMAGES {
                wanted.push(image.source);
            }
        }
        for &source in &wanted {
            if let Some(cached) = self.cache.iter_mut().find(|cached| cached.source == source) {
                cached.used = self.clock;
                continue;
            }
            while self.cache.len() >= MAX_IMAGES {
                let candidate = self
                    .cache
                    .iter()
                    .enumerate()
                    .filter(|(_, cached)| !wanted.contains(&cached.source))
                    .min_by_key(|(_, cached)| cached.used)
                    .map(|(index, _)| index);
                let Some(index) = candidate else { break };
                self.retire(index);
            }
            if self.cache.len() >= MAX_IMAGES
                || self.cache.len() + self.retired.len() >= MAX_OWNED_IDS
            {
                continue;
            }
            let Some(id) = fresh_id() else { continue };
            self.cache.push(Cached {
                source,
                raster: self.sources[source].clone(),
                id,
                upload: State::New,
                placements: Vec::new(),
                used: self.clock,
            });
        }
        // Every delete precedes every upload, including all chunks of an upload.
        // A delete during chunked transmission aborts it in Kitty and Ghostty.
        self.deletions(&mut output);
        for cached in &mut self.cache {
            if wanted.contains(&cached.source) && matches!(cached.upload, State::New) {
                upload(&mut output, cached);
                cached.upload = State::Emitted;
            }
        }
        let mut cells = 0;
        for (image, clip) in visible {
            let Some(cached) = self
                .cache
                .iter_mut()
                .find(|cached| cached.source == image.source)
            else {
                continue;
            };
            if !wanted.contains(&image.source) || !matches!(cached.upload, State::Ready) {
                continue;
            }
            let placement = cached.placements.iter().position(|p| p.grid == image.grid);
            let placement = match placement {
                Some(index) => index,
                None if cached.placements.len() < MAX_PLACEMENTS => {
                    cached.placements.push(Placement {
                        id: (cached.placements.len() + 1) as u32,
                        grid: image.grid,
                        state: State::New,
                    });
                    cached.placements.len() - 1
                }
                None => continue,
            };
            let placement = &mut cached.placements[placement];
            match placement.state {
                State::New => {
                    let _ = write!(
                        output,
                        "{APC}a=p,U=1,i={},p={},c={},r={},C=1,q=0;{ST}",
                        cached.id, placement.id, placement.grid.columns, placement.grid.rows
                    );
                    placement.state = State::Emitted;
                }
                State::Ready => {
                    let count = (clip.bottom - clip.top) * (clip.right - clip.left);
                    if count <= MAX_DRAW_CELLS - cells {
                        overlay(&mut output, cached.id, placement.id, image, clip, view);
                        cells += count;
                    }
                }
                State::Emitted | State::Waiting(_) => {}
            }
        }
        output
    }

    /// True only when an expected, currently owned reply changes state. Uploads
    /// expect no placement ID (or zero); placement replies require their exact p.
    /// Unknown, duplicate, retired and late successful ACKs cannot revive an asset.
    pub(super) fn reply(
        &mut self,
        image: u32,
        placement: Option<u32>,
        ok: bool,
        now: Instant,
    ) -> bool {
        let Some(index) = self.cache.iter().position(|cached| cached.id == image) else {
            return false;
        };
        let cached = &mut self.cache[index];
        let state = if matches!(cached.upload, State::Waiting(_)) && placement.unwrap_or(0) == 0 {
            &mut cached.upload
        } else if let Some(placement) = placement.and_then(|id| {
            cached
                .placements
                .iter_mut()
                .find(|p| p.id == id && matches!(p.state, State::Waiting(_)))
        }) {
            &mut placement.state
        } else {
            return false;
        };
        let expired = state.deadline().is_some_and(|deadline| now >= deadline);
        if !ok || expired {
            self.disable();
        } else {
            *state = State::Ready;
        }
        true
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.cache
            .iter()
            .flat_map(|cached| {
                std::iter::once(cached.upload.deadline()).chain(
                    cached
                        .placements
                        .iter()
                        .map(|placement| placement.state.deadline()),
                )
            })
            .flatten()
            .min()
    }

    /// Include retired IDs until their deletion has actually been flushed. Keep
    /// this entire list in the caller's panic-cleanup state before writing output.
    pub(super) fn owned_ids(&self) -> Vec<u32> {
        self.cache
            .iter()
            .map(|cached| cached.id)
            .chain(self.retired.iter().map(|retired| retired.id))
            .collect()
    }

    /// Start ACK deadlines only after the caller's successful write/flush. A large
    /// upload may take longer than REPLY_WAIT to encode/write; that is not an ACK
    /// timeout. Already waiting commands keep their original deadlines on redraw.
    /// Commit only deletions emitted before the caller's successful write/flush.
    /// A reset that happened since frame() can queue additional un-emitted IDs;
    /// those must remain registered even when written() commits the earlier frame.
    pub(super) fn written(&mut self) {
        let now = Instant::now();
        for cached in &mut self.cache {
            cached.upload.written(now);
            for placement in &mut cached.placements {
                placement.state.written(now);
            }
        }
        self.retired.retain(|retired| !retired.emitted);
    }

    /// Terminal cleanup is intentionally explicit, repeatable, and ID-scoped.
    /// Keep IDs registered until the cleanup string has been flushed + written().
    pub(super) fn cleanup(&mut self) -> String {
        self.disable();
        let mut output = String::new();
        self.deletions(&mut output);
        output
    }

    fn retire(&mut self, index: usize) {
        let cached = self.cache.remove(index);
        self.retired.push(Retired {
            id: cached.id,
            emitted: false,
        });
    }

    fn disable(&mut self) {
        self.disabled = true;
        while !self.cache.is_empty() {
            self.retire(self.cache.len() - 1);
        }
        self.sources.clear();
        self.scene.clear();
    }

    fn expire(&mut self, now: Instant) {
        if self.deadline().is_some_and(|deadline| now >= deadline) {
            self.disable();
        }
    }

    fn deletions(&mut self, output: &mut String) {
        for retired in &mut self.retired {
            let _ = write!(output, "{APC}a=d,d=I,i={},q=2;{ST}", retired.id);
            retired.emitted = true;
        }
    }
}

fn pixel_bytes(image: &Image) -> Option<usize> {
    if image.width == 0 || image.height == 0 {
        return None;
    }
    let size = usize::try_from(image.width)
        .ok()?
        .checked_mul(usize::try_from(image.height).ok()?)?
        .checked_mul(4)?;
    (size <= MAX_BYTES && size == image.pixels.len()).then_some(size)
}

// IDs are process-wide monotonic and never reused or wrapped. A per-process seed
// reduces collision risk with unrelated clients in the terminal's global namespace.
// This is allocation, not a claim that other terminal clients share our allocator.
fn fresh_id() -> Option<u32> {
    static NEXT: OnceLock<AtomicU64> = OnceLock::new();
    let next = NEXT.get_or_init(|| {
        let mut hasher = DefaultHasher::new();
        SystemTime::now().hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        AtomicU64::new((hasher.finish() & 0x7fff_ffff) + 1)
    });
    let mut value = next.load(Ordering::Relaxed);
    loop {
        if value >= u64::from(u32::MAX) {
            return None;
        }
        match next.compare_exchange_weak(value, value + 1, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return u32::try_from(value).ok(),
            Err(current) => value = current,
        }
    }
}

fn upload(output: &mut String, cached: &Cached) {
    let chunks = cached.raster.pixels.chunks(RAW_CHUNK);
    let count = chunks.len();
    for (index, bytes) in chunks.enumerate() {
        output.push_str(APC);
        if index == 0 {
            let _ = write!(
                output,
                "a=t,t=d,f=32,s={},v={},i={},q=0,",
                cached.raster.width, cached.raster.height, cached.id
            );
        }
        let _ = write!(
            output,
            "m={};{}{ST}",
            u8::from(index + 1 < count),
            base64::encode(bytes)
        );
    }
}

fn overlay(
    output: &mut String,
    image_id: u32,
    placement_id: u32,
    image: SceneImage,
    clip: Clip,
    view: Viewport,
) {
    for row in clip.top..clip.bottom {
        // Reset attributes/background before encoding IDs, and again after each
        // fragment: transparent pixels must show the default terminal background,
        // never the half-block fallback's old background or a search inversion.
        let _ = write!(
            output,
            "\x1b[{};{}H{RESET}\x1b[38;2;{};{};{}m\x1b[58;2;{};{};{}m",
            row - view.top + 1,
            clip.left - view.left + 1,
            (image_id >> 16) & 255,
            (image_id >> 8) & 255,
            image_id & 255,
            (placement_id >> 16) & 255,
            (placement_id >> 8) & 255,
            placement_id & 255
        );
        for column in clip.left..clip.right {
            output.push(PLACEHOLDER);
            output.push(DIACRITICS[row - image.line]);
            output.push(DIACRITICS[column - image.column]);
            output.push(DIACRITICS[(image_id >> 24) as usize]);
        }
        output.push_str(RESET);
    }
}

// Exact index order from the primary protocol's rowcolumn-diacritics.txt,
// independently matched to Ghostty v1.4.0 graphics_unicode.zig (all 297 entries).
// https://sw.kovidgoyal.net/kitty/_downloads/f0a0de9ec8d9ff4456206db8e0814937/rowcolumn-diacritics.txt
const DIACRITICS: &[char] = &[
    '\u{305}',
    '\u{30d}',
    '\u{30e}',
    '\u{310}',
    '\u{312}',
    '\u{33d}',
    '\u{33e}',
    '\u{33f}',
    '\u{346}',
    '\u{34a}',
    '\u{34b}',
    '\u{34c}',
    '\u{350}',
    '\u{351}',
    '\u{352}',
    '\u{357}',
    '\u{35b}',
    '\u{363}',
    '\u{364}',
    '\u{365}',
    '\u{366}',
    '\u{367}',
    '\u{368}',
    '\u{369}',
    '\u{36a}',
    '\u{36b}',
    '\u{36c}',
    '\u{36d}',
    '\u{36e}',
    '\u{36f}',
    '\u{483}',
    '\u{484}',
    '\u{485}',
    '\u{486}',
    '\u{487}',
    '\u{592}',
    '\u{593}',
    '\u{594}',
    '\u{595}',
    '\u{597}',
    '\u{598}',
    '\u{599}',
    '\u{59c}',
    '\u{59d}',
    '\u{59e}',
    '\u{59f}',
    '\u{5a0}',
    '\u{5a1}',
    '\u{5a8}',
    '\u{5a9}',
    '\u{5ab}',
    '\u{5ac}',
    '\u{5af}',
    '\u{5c4}',
    '\u{610}',
    '\u{611}',
    '\u{612}',
    '\u{613}',
    '\u{614}',
    '\u{615}',
    '\u{616}',
    '\u{617}',
    '\u{657}',
    '\u{658}',
    '\u{659}',
    '\u{65a}',
    '\u{65b}',
    '\u{65d}',
    '\u{65e}',
    '\u{6d6}',
    '\u{6d7}',
    '\u{6d8}',
    '\u{6d9}',
    '\u{6da}',
    '\u{6db}',
    '\u{6dc}',
    '\u{6df}',
    '\u{6e0}',
    '\u{6e1}',
    '\u{6e2}',
    '\u{6e4}',
    '\u{6e7}',
    '\u{6e8}',
    '\u{6eb}',
    '\u{6ec}',
    '\u{730}',
    '\u{732}',
    '\u{733}',
    '\u{735}',
    '\u{736}',
    '\u{73a}',
    '\u{73d}',
    '\u{73f}',
    '\u{740}',
    '\u{741}',
    '\u{743}',
    '\u{745}',
    '\u{747}',
    '\u{749}',
    '\u{74a}',
    '\u{7eb}',
    '\u{7ec}',
    '\u{7ed}',
    '\u{7ee}',
    '\u{7ef}',
    '\u{7f0}',
    '\u{7f1}',
    '\u{7f3}',
    '\u{816}',
    '\u{817}',
    '\u{818}',
    '\u{819}',
    '\u{81b}',
    '\u{81c}',
    '\u{81d}',
    '\u{81e}',
    '\u{81f}',
    '\u{820}',
    '\u{821}',
    '\u{822}',
    '\u{823}',
    '\u{825}',
    '\u{826}',
    '\u{827}',
    '\u{829}',
    '\u{82a}',
    '\u{82b}',
    '\u{82c}',
    '\u{82d}',
    '\u{951}',
    '\u{953}',
    '\u{954}',
    '\u{f82}',
    '\u{f83}',
    '\u{f86}',
    '\u{f87}',
    '\u{135d}',
    '\u{135e}',
    '\u{135f}',
    '\u{17dd}',
    '\u{193a}',
    '\u{1a17}',
    '\u{1a75}',
    '\u{1a76}',
    '\u{1a77}',
    '\u{1a78}',
    '\u{1a79}',
    '\u{1a7a}',
    '\u{1a7b}',
    '\u{1a7c}',
    '\u{1b6b}',
    '\u{1b6d}',
    '\u{1b6e}',
    '\u{1b6f}',
    '\u{1b70}',
    '\u{1b71}',
    '\u{1b72}',
    '\u{1b73}',
    '\u{1cd0}',
    '\u{1cd1}',
    '\u{1cd2}',
    '\u{1cda}',
    '\u{1cdb}',
    '\u{1ce0}',
    '\u{1dc0}',
    '\u{1dc1}',
    '\u{1dc3}',
    '\u{1dc4}',
    '\u{1dc5}',
    '\u{1dc6}',
    '\u{1dc7}',
    '\u{1dc8}',
    '\u{1dc9}',
    '\u{1dcb}',
    '\u{1dcc}',
    '\u{1dd1}',
    '\u{1dd2}',
    '\u{1dd3}',
    '\u{1dd4}',
    '\u{1dd5}',
    '\u{1dd6}',
    '\u{1dd7}',
    '\u{1dd8}',
    '\u{1dd9}',
    '\u{1dda}',
    '\u{1ddb}',
    '\u{1ddc}',
    '\u{1ddd}',
    '\u{1dde}',
    '\u{1ddf}',
    '\u{1de0}',
    '\u{1de1}',
    '\u{1de2}',
    '\u{1de3}',
    '\u{1de4}',
    '\u{1de5}',
    '\u{1de6}',
    '\u{1dfe}',
    '\u{20d0}',
    '\u{20d1}',
    '\u{20d4}',
    '\u{20d5}',
    '\u{20d6}',
    '\u{20d7}',
    '\u{20db}',
    '\u{20dc}',
    '\u{20e1}',
    '\u{20e7}',
    '\u{20e9}',
    '\u{20f0}',
    '\u{2cef}',
    '\u{2cf0}',
    '\u{2cf1}',
    '\u{2de0}',
    '\u{2de1}',
    '\u{2de2}',
    '\u{2de3}',
    '\u{2de4}',
    '\u{2de5}',
    '\u{2de6}',
    '\u{2de7}',
    '\u{2de8}',
    '\u{2de9}',
    '\u{2dea}',
    '\u{2deb}',
    '\u{2dec}',
    '\u{2ded}',
    '\u{2dee}',
    '\u{2def}',
    '\u{2df0}',
    '\u{2df1}',
    '\u{2df2}',
    '\u{2df3}',
    '\u{2df4}',
    '\u{2df5}',
    '\u{2df6}',
    '\u{2df7}',
    '\u{2df8}',
    '\u{2df9}',
    '\u{2dfa}',
    '\u{2dfb}',
    '\u{2dfc}',
    '\u{2dfd}',
    '\u{2dfe}',
    '\u{2dff}',
    '\u{a66f}',
    '\u{a67c}',
    '\u{a67d}',
    '\u{a6f0}',
    '\u{a6f1}',
    '\u{a8e0}',
    '\u{a8e1}',
    '\u{a8e2}',
    '\u{a8e3}',
    '\u{a8e4}',
    '\u{a8e5}',
    '\u{a8e6}',
    '\u{a8e7}',
    '\u{a8e8}',
    '\u{a8e9}',
    '\u{a8ea}',
    '\u{a8eb}',
    '\u{a8ec}',
    '\u{a8ed}',
    '\u{a8ee}',
    '\u{a8ef}',
    '\u{a8f0}',
    '\u{a8f1}',
    '\u{aab0}',
    '\u{aab2}',
    '\u{aab3}',
    '\u{aab7}',
    '\u{aab8}',
    '\u{aabe}',
    '\u{aabf}',
    '\u{aac1}',
    '\u{fe20}',
    '\u{fe21}',
    '\u{fe22}',
    '\u{fe23}',
    '\u{fe24}',
    '\u{fe25}',
    '\u{fe26}',
    '\u{10a0f}',
    '\u{10a38}',
    '\u{1d185}',
    '\u{1d186}',
    '\u{1d187}',
    '\u{1d188}',
    '\u{1d189}',
    '\u{1d1aa}',
    '\u{1d1ab}',
    '\u{1d1ac}',
    '\u{1d1ad}',
    '\u{1d242}',
    '\u{1d243}',
    '\u{1d244}',
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_preserves_high_id_byte_placement_color_and_sgr_boundaries() {
        // Explicit IDs make the high-byte test independent of the random allocator
        // seed. Expectations are primary-protocol codepoints, not table lookups.
        for (id, foreground, high) in [
            (0x0212_3456, "18;52;86", '\u{30e}'),
            (0xffab_cdef, "171;205;239", '\u{a8e5}'),
        ] {
            let mut output = String::new();
            overlay(
                &mut output,
                id,
                0x00a1_b2c3,
                SceneImage {
                    source: 0,
                    line: 7,
                    column: 11,
                    grid: Grid {
                        columns: 4,
                        rows: 3,
                    },
                },
                Clip {
                    top: 8,
                    bottom: 9,
                    left: 13,
                    right: 14,
                },
                Viewport {
                    top: 8,
                    left: 13,
                    rows: 1,
                    columns: 1,
                },
            );
            assert_eq!(
                output,
                format!(
                    "\x1b[1;1H\x1b[0m\x1b[38;2;{foreground}m\x1b[58;2;161;178;195m\u{10eeee}\u{30d}\u{30e}{high}\x1b[0m"
                )
            );
        }
    }
}
