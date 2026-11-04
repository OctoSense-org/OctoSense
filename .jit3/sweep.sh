#!/bin/zsh
R=/Users/ychen/home/octosense-org/.worktrees/jit3-work/target/release/relax
OUT=/Users/ychen/home/octosense-org/.worktrees/jit3-octosense/.jit3/sweep-results.tsv
: > $OUT
while IFS=$'\t' read -r name dir; do
  cd "$dir" || { printf "%s\tNODIR\t0s\t\t\n" "$name" >> $OUT; continue; }
  t0=$(date +%s)
  out=$(MAKEPAD_RUN_ALLOW_C=1 nice perl -e 'alarm 300; exec @ARGV' -- "$R" check "$name" 2>&1)
  rc=$?; t1=$(date +%s)
  load=$(printf "%s" "$out" | grep -m1 -E 'load error|panicked|build script failed' | cut -c1-200 | tr '\t' ' ')
  summ=$(printf "%s" "$out" | grep -m1 -oE '[0-9]+ errors' )
  firsterr=$(printf "%s" "$out" | grep -m1 -E ': E[0-9]{4}:|error:' | sed -E 's|.*/src/||' | cut -c1-160 | tr '\t' ' ')
  if [ -n "$load" ]; then st=LOAD; elif [ $rc -eq 142 ] || [ $rc -eq 14 ]; then st=TIMEOUT; elif [ "$summ" = "0 errors" ]; then st=OK; elif [ -n "$summ" ]; then st=TYPE; else st="RC$rc"; fi
  printf "%s\t%s\t%ss\t%s\t%s\n" "$name" "$st" "$((t1-t0))" "${summ:-}" "${load:-$firsterr}" >> $OUT
done < /Users/ychen/home/octosense-org/.worktrees/jit3-octosense/.jit3/sweep-list.tsv
echo "sweep done $(date -u +%FT%TZ)" >> $OUT
