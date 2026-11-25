#!/usr/bin/env bash
# Runs every scenario once and prints a summary. PREFIX names the runs (default: run).
set -u
here=$(cd "$(dirname "$0")" && pwd)
work=${PERF_DIR:-$HOME/.cache/desafinado-perf}
p=${PREFIX:-run}
mkdir -p "$work/docs" "$work/ud-nospell"
[ -e "$work/docs/huge.md" ] || python3 "$here/gen_docs.py" "$work/docs"
echo '{"enabled": false}' > "$work/ud-nospell/spelling.json"
r() { "$here/run.sh" "$p-$1" "$work/docs/$2.md" "$here/scenarios/$3.txt" ${BIN:+"$BIN"} 2>&1 | grep done; }
for s in small medium large huge; do r "startup-$s" "$s" startup; done
r idle-medium medium idle
for s in small medium large huge; do r "type-$s" "$s" type; done
SEED_UD=$work/ud-nospell r type-nospell-large large type
SEED_UD=$work/ud-nospell r type-nospell-huge huge type
r type-paragraph paragraph type
r edit-large large edit
r scroll-large large scroll
r scroll-huge huge scroll
r jump-huge huge jump
r reading-large large reading
r library-medium medium library
r ask-medium medium ask
r ask-huge huge ask
r review_suggest-medium medium review_suggest
r review_notes-medium medium review_notes
r replace_all-large large replace_all
r save-large large save
r resize-large large resize
python3 "$here/analyze.py" "$work/runs/$p"-* > "$work/summary-$p.txt"
echo "summary in $work/summary-$p.txt"
