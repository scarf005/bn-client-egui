#!/bin/sh
# Extracts the client's own words (`tr`, `tr_format`, `tr_plural`) into crates/core/po/bn-client.pot and merges
# them into every crates/core/po/<locale>.po.
set -eu
cd "$(dirname "$0")/.."
po=crates/core/po
git ls-files 'crates/*/src/*.rs' 'crates/*/src/**/*.rs' | xgettext -f - -L Rust --from-code=UTF-8 \
    --keyword=tr --keyword=tr_format --keyword=tr_plural:1,2 \
    --flag=tr_format:1:c-format --flag=tr_plural:1:c-format --flag=tr_plural:2:c-format \
    --add-comments=TRANSLATORS: --sort-by-file --package-name=bn-client -o "$po/bn-client.pot"
for file in "$po"/*.po; do
    msgmerge --quiet --update --backup=none "$file" "$po/bn-client.pot"
done
