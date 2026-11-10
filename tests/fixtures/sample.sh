#!/bin/sh
# sample.sh: a script with no Markdown in it
set -eu

for f in *.md; do
  echo "* $f"
done
