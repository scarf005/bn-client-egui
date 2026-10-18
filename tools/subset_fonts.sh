#!/bin/sh
# Writes crates/egui/assets/fonts/: Noto Sans CJK KR (proportional) and Noto Sans Mono CJK KR (monospace), cut to
# Latin, punctuation, symbols, kana and Hangul, with their OFL licence. Only the proportional face keeps hanja/kanji
# (the KS X 1001 set it carries); monospace text falls back to it for them. Their 1.45 em line (egui rows take the
# first face's) is cut to 1.2 em, about the line of the Latin faces they replace.
set -eu
cd "$(dirname "$0")/.."
release=https://github.com/notofonts/noto-cjk/raw/Sans2.004
out=crates/egui/assets/fonts
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"
curl -fsSL -o "$work/sans.otf" "$release/Sans/SubsetOTF/KR/NotoSansKR-Regular.otf"
curl -fsSL -o "$work/mono.otf" "$release/Sans/Mono/NotoSansMonoCJKkr-Regular.otf"
curl -fsSL -o "$out/LICENSE" "$release/LICENSE"
ranges=U+0020-007E,U+00A0-024F,U+0370-03FF,U+0400-04FF,U+1100-11FF,U+2000-206F,U+2070-209F,U+20A0-20CF,\
U+2100-218F,U+2190-21FF,U+2200-22FF,U+2300-23FF,U+2460-24FF,U+2500-259F,U+25A0-25FF,U+2600-26FF,U+3000-303F,\
U+3040-30FF,U+3130-318F,U+3200-32FF,U+AC00-D7A3,U+FF00-FFEF
line='
import sys
from fontTools.ttLib import TTFont

font = TTFont(sys.argv[1])
hhea, os2 = font["hhea"], font["OS/2"]
hhea.ascent, hhea.descent, hhea.lineGap = 1000, -200, 0
os2.sTypoAscender, os2.sTypoDescender, os2.sTypoLineGap = 1000, -200, 0
os2.usWinAscent, os2.usWinDescent = 1000, 200
font.save(sys.argv[1])
'
subset() {
    uvx --from fonttools==4.60.1 pyftsubset "$work/$1.otf" --unicodes="$2" --layout-features='*' --no-hinting \
        --output-file="$out/$1.otf"
    uvx --from fonttools==4.60.1 python -c "$line" "$out/$1.otf"
}
subset sans "$ranges,U+4E00-9FFF"
subset mono "$ranges"
