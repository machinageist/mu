===========
 frobnicate
===========

.. image:: https://img.shields.io/pypi/v/frobnicate.svg
   :alt: PyPI version
   :target: https://pypi.org/project/frobnicate/

:Author: Jane Doe
:Version: 2.4.1
:License: MIT, see the LICENSE file for the
          full text

**frobnicate** turns *ordinary* data into ``frobnicated`` data. It is fast,
small and well tested. Read the docs at `Read the Docs <https://frobnicate.readthedocs.io/>`_
or browse the source on `GitHub <https://github.com/example/frobnicate>`__.
Bare links such as https://example.com stay as text, and |badge| substitutions too.

.. contents:: Table of Contents
   :depth: 2

.. _installation:

Installation
============

Install from PyPI with pip::

    pip install frobnicate

Or build from a checkout ::

    git clone https://github.com/example/frobnicate
    cd frobnicate
    pip install -e .

.. note::
   Python 3.9 or newer is required. Older interpreters are not supported.

.. warning:: Version 2 drops the ``legacy`` module.

Quick start
===========

Import the package and call :func:`frobnicate.run` with a path:

.. code-block:: python
   :linenos:

   from frobnicate import run

   result = run("data.csv", strict=True)
   print(result.summary())

The command line tool works too:

.. code:: console

   $ frob data.csv --strict

Features
--------

- Reads CSV, JSON and Parquet
- Streams large inputs in constant memory
- Pluggable backends:

  * ``numpy`` for speed
  * ``pure`` for portability

- Works on Linux, macOS and Windows

Steps to release:

1. Bump the version in ``pyproject.toml``
2. Update ``CHANGELOG.rst``
#. Tag the release and push

Configuration
-------------

strict
    Fail on the first malformed row instead of skipping it.

encoding : str
    Text encoding of the input. Defaults to UTF-8.

workers
    Number of worker threads.

    Use ``0`` to pick one per CPU core.

Backends
~~~~~~~~

=========  ===========  ========
Backend    Speed        Requires
=========  ===========  ========
numpy      fast         numpy
pure       slow         nothing
arrow      fastest      pyarrow,
                        numpy
=========  ===========  ========

+-------------+----------------------+
| Option      | Meaning              |
+=============+======================+
| ``--strict``| Stop on bad rows     |
+-------------+----------------------+
| ``--quiet`` | Print nothing but    |
|             | errors               |
+-------------+----------------------+

.. tip:: Prefer ``arrow`` when pyarrow is installed.

.. admonition:: Thread safety

   Each worker owns its own parser, so no locks are needed.

Caveats
~~~~~~~

    Premature optimization is the root of all evil.

    -- Donald Knuth

.. danger::
   Never point frobnicate at ``/dev/sda``.

.. hint:: Run with ``--dry-run`` first.

.. important:: Back up your data.

.. caution:: Output files are overwritten.

----

Contributing
============

See `the guide`_ and :ref:`Contributing <contributing>` for details.
Use the ``make test`` target before opening a pull request.

.. This is a comment and does not render.
   Neither does its second line.

.. toctree::
   :maxdepth: 1

   changelog
   api

.. [1] Footnote text for the curious.

.. _the guide: https://example.com/guide
.. |badge| image:: https://example.com/badge.svg
