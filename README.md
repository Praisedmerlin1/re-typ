# re-typ

**Convert LaTeX math and TikZ diagrams in Markdown to Typst — without touching the rest of the file.**

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)
[![Typst](https://img.shields.io/badge/output-Typst-239DAD.svg)](https://typst.app)

`re-typ` is a surgical rewriter for Markdown notes. It changes exactly two
things and copies everything else through byte-for-byte:

| Input | Output |
|-------|--------|
| `$…$` / `$$…$$` LaTeX math | native Typst math |
| ` ```tikz ` / ` ```latex ` blocks containing a `tikzpicture` | CeTZ code **or** an SVG image |

This makes Obsidian notes render with the [TypstMate] plugin without a LaTeX
installation, while keeping headings, lists, tables, embeds, callouts and
frontmatter exactly as written.

Math conversion is powered by [TyLax], the same converter TypstMate uses
internally, so the output is standalone Typst that needs no package preamble.

A secondary `--format typ` mode emits a complete `.typ` document for when you
need fine-grained control over a PDF build.

## Features

- **Non-destructive by default.** Writes a sibling `.typst.md`; the original is
  untouched unless you pass `--in-place`. The `md` rewriter works on source
  byte ranges, so only math spans and TikZ fences are modified.
- **Native Typst math.** `\sqrt{x}` → `sqrt(x)`, `\mathbb{R}` → `RR`,
  `\begin{pmatrix}…` → `mat(delim: "("…)`, and so on.
- **TikZ → CeTZ or SVG.** Auto-detects constructs CeTZ cannot handle and falls
  back to rendering an SVG, with an empty-output guard for safety.
- **Honest failure.** A diagram that fails to render is left unchanged and
  reported; the rest of the file still converts. `--strict` makes it fatal.
- **Dry run and dependency check.** `--dry-run` previews everything and invokes
  no external tools; `--check-deps` reports your toolchain.
- **Recursive.** Point it at a vault and it finds every `.md` file.
- **Safe to re-run.** Already-converted Typst math is detected and left alone,
  and notes that already contain `typst`/`cetz` blocks are skipped.

## Requirements

Math conversion and CeTZ output need **no external tools**.

A TeX engine and a PDF → SVG backend are needed only when a diagram is
rendered to an image:

- **TeX engine** (one of): `tectonic`, `pdflatex`, `xelatex`, `lualatex`
- **SVG backend** (one of): `pdftocairo`, `dvisvgm`, `pdf2svg`, `inkscape`

Check what is available:

```console
$ re-typ --check-deps
external toolchain:
  tex engine : tectonic   /home/you/.cargo/bin/tectonic  (Tectonic 0.17.0)
  svg backend: pdftocairo /usr/bin/pdftocairo  (pdftocairo version 26.08.0)
```

`tectonic` is the easiest choice: it is self-contained and downloads the
packages it needs on first use. Installed-but-broken engines (for example a
TeX Live install missing its format files) are detected and skipped
automatically.

## Installation

Requires Rust 1.88 or newer.

```console
$ git clone https://github.com/<you>/re-typ
$ cd re-typ
$ cargo install --path .
```

Or build without installing and use `target/release/re-typ`:

```console
$ cargo build --release
```

## Quick start

Given `notes.md`:

````markdown
# Physics

Inline $E = mc^2$.

$$
\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}
$$

```tikz
\begin{tikzpicture}
\draw (0,0) circle (1cm);
\draw (0,0) rectangle (2,1);
\end{tikzpicture}
```
````

Preview, then convert:

```console
$ re-typ --dry-run notes.md
notes.md
  math: 2/2 would convert  (0 failed)
  tikz: 1 block(s), 1 -> CeTZ, 0 -> SVG
  would write: ./notes.typst.md

summary: 1 file(s), math 2/2, tikz blocks 1 (CeTZ 1, SVG 0), warnings 0
         (dry run: no files written, no TeX invoked)

$ re-typ notes.md
```

`notes.typst.md` now contains:

````markdown
# Physics

Inline $E = m c^(2)$.

$$integral_(0)^(infinity) e^(- x^(2))thin d x = frac(sqrt(pi), 2)$$

```typst
#import "@preview/cetz:0.3.4": canvas, draw

#canvas({
  import draw: *

  circle((0, 0), radius: 1)
  rect((0, 0), (2, 1))
})
```
````

Everything except the two math spans and the diagram is identical to the
original.

## Usage

```console
# Convert one file (writes a sibling .typst.md)
re-typ notes.md

# Convert a whole vault, recursively
re-typ ~/vault

# Preview only
re-typ --dry-run ~/vault

# Rewrite the originals in place (back up first)
re-typ --in-place notes.md

# Emit a full Typst document instead of Markdown
re-typ --format typ notes.md          # -> notes.typ

# Redirect outputs and their SVG assets
re-typ -o build notes.md

# Force a diagram backend
re-typ --tikz-backend cetz notes.md   # CeTZ only
re-typ --tikz-backend svg  notes.md   # images only
```

### Options

| Flag | Description |
|------|-------------|
| `<INPUT>…` | `.md` files or directories (searched recursively) |
| `--format <md\|typ>` | Output flavor. `md` (default) is Obsidian-friendly; `typ` also translates Markdown structure |
| `-o, --output-dir <DIR>` | Directory for outputs and SVG assets. Default: next to each input |
| `--suffix <S>` | Sibling name suffix for Markdown output. Default `.typst` → `notes.typst.md` |
| `--in-place` | Overwrite the input file (Markdown output only) |
| `--force` | Rebuild SVG assets even if they already exist |
| `--no-skip-typst-files` | Also process files that already contain `typst`/`cetz` blocks (skipped by default) |
| `--strict` | Exit non-zero on any warning (failed math, dropped HTML, failed render) |
| `-v, --verbose` | Print per-file details, including non-fatal math notes |
| `--dry-run` | Parse and report; write nothing, invoke no external tools |
| `--check-deps` | Print the detected toolchain and exit |
| `--tikz-backend <auto\|cetz\|svg>` | How TikZ blocks are rendered. Default `auto` |
| `--svg-backend <auto\|dvisvgm\|pdftocairo\|pdf2svg\|inkscape>` | PDF → SVG backend |
| `--tex-engine <auto\|pdflatex\|xelatex\|lualatex\|tectonic>` | TeX engine used for SVG builds |
| `--tikz-preamble <FILE>` | Extra LaTeX preamble injected into SVG builds |

Run `re-typ --help` for the complete reference.

## Output modes

### `--format md` (default)

Only math spans and TikZ fences change. SVG fallbacks become Obsidian embeds:

```markdown
![[notes-1.svg]]
```

The SVG is written next to the output as `notes-1.svg`, `notes-2.svg`, … in
document order. Move it wherever your vault expects and adjust the embed path
if needed.

### `--format typ`

Also translates Markdown structure into Typst: headings (`#` → `=`), emphasis,
lists, blockquotes (`#quote`), tables (`#table`), links (`#link`), images and
code blocks. TikZ becomes inline `#canvas(...)` or `#image("notes-1.svg")`.

Because Markdown is not valid Typst in general, use `--format md` for
Obsidian/TypstMate and `--format typ` for the Typst compiler.

## Skipping notes that already use Typst

Notes that already contain a ` ```typst ` or ` ```cetz ` fenced block are
treated as migrated and **skipped entirely** (no output written), so re-running
over a mix of converted and unconverted notes is safe. Pass
`--no-skip-typst-files` to force processing anyway.

`.typ` files are never picked up as inputs; only `.md` and `.markdown` are.

## How diagrams are rendered

`--tikz-backend auto` decides per block:

1. **CeTZ** when TyLax can transpile the block (basic shapes, paths, nodes,
   coordinates, `\foreach`, common styles). Embedded as a ` ```typst ` fence in
   Markdown, or inline `#canvas(...)` in `.typ`.
2. **SVG** for constructs the transpiler silently drops: `circuitikz`,
   pgfplots (`axis` / `\addplot`), `tikzcd`, `\tikzset`, `\newcommand`,
   `\pgfmath`. Missing LaTeX packages (`circuitikz`, `pgfplots`, `tikz-cd`) are
   injected automatically.
3. **SVG** if a CeTZ conversion produces an empty canvas.

Force either behaviour with `--tikz-backend cetz` or `--tikz-backend svg`.

> **CeTZ output is best-effort.** `re-typ` does not visually verify diagrams.
> Review converted diagrams and switch to `--tikz-backend svg` if one looks
> wrong.

## Troubleshooting

**`no TeX engine found` / SVG build fails.** Run `re-typ --check-deps` and
install `tectonic` (or a full TeX Live). If your system `pdflatex` is broken,
force the working engine with `--tex-engine tectonic`.

**A single diagram fails to render.** The block is left exactly as it was, a
warning is printed, and the rest of the file is still converted. Pass
`--strict` to make the whole run fail instead. Snippets that paste a full
document body (`\begin{document}` … `\end{document}` without a `\documentclass`)
are unwrapped automatically.

**`unknown variable: ...` when compiling a `.typ`.** A Markdown construct had
no Typst equivalent. Run without `--strict` and check the reported warnings.

**"N non-fatal math note(s)".** These are not errors: for example, LaTeX allows
a superscript or subscript with no base (`cm$^3$`, `$_{D}$`) where Typst does
not. `re-typ` repairs these automatically (`""^(3)`, `""_(D)`) and reports a
note. Details are hidden by default; pass `--verbose` to list them. They never
fail a run, with or without `--strict`.

**Can I run it twice?** It is safe to re-run: already-converted Typst math is
detected and left alone, so `beta` will not become `b e t a`. The only known
exception is a bare slash fraction, which a second pass may rewrite from `a/b`
to `a slash b`. Prefer converting once and keeping a backup.

**A CeTZ block renders blank.** The input used something the transpiler dropped.
Re-run with `--tikz-backend svg`.

**First SVG conversion is slow.** `tectonic` downloads packages on first use;
the tool allows up to five minutes per compile for this.

**TypstMate cannot find the CeTZ import.** It needs network access once to fetch
`@preview/cetz`; the version is pinned by TyLax (currently `0.3.4`).

## Development

```console
$ cargo test
$ cargo fmt --check
$ cargo clippy --all-targets -- -D warnings

# End-to-end TikZ -> SVG (requires a working TeX toolchain):
$ cargo test --test tikz_toolchain -- --ignored --nocapture
```

Source layout:

```
src/
  main.rs      CLI entry point
  cli.rs       argument definitions
  config.rs    resolved options
  lib.rs       run() and the in-memory conversion entry point
  deps.rs      toolchain detection and timeouts
  math.rs      LaTeX -> Typst math (TyLax)
  tikz.rs      CeTZ transpile + LaTeX/SVG compilation
  context.rs   per-file state and backend decision
  rewrite.rs   Markdown mode (byte-preserving)
  markdown.rs  Typst mode (structural translation)
  assets.rs    SVG naming
  fsutil.rs    input discovery and output paths
  report.rs    stats, warnings, summaries
tests/         integration tests and fixtures
```

## Contributing

Issues and pull requests are welcome. Please run `cargo fmt`, `cargo clippy
--all-targets -- -D warnings` and `cargo test` before opening a PR. Add a
fixture under `tests/fixtures/` for any behavior change.

## License

Licensed under the [MIT License](LICENSE).

## Acknowledgements

- [TyLax] — LaTeX ↔ Typst conversion (math and CeTZ)
- [TypstMate] — the Obsidian plugin this targets
- [pulldown-cmark] — Markdown parsing

[TyLax]: https://github.com/scipenai/tylax
[TypstMate]: https://github.com/azyarashi/obsidian-typst-mate
[pulldown-cmark]: https://github.com/raphlinus/pulldown-cmark
