#!/usr/bin/env nu

def main [--min: float = 67.0] {
   let totals = (
      ^cargo llvm-cov --locked --all-features --branch --json --summary-only
      | from json
      | get data.0.totals
   )
   let branches = $totals.branches.percent | math round --precision 2
   let lines = $totals.lines.percent | math round --precision 2

   print $"branches ($branches)%, lines ($lines)%"

   if $branches < $min {
      error make { msg: $"branch coverage ($branches)% is under ($min)%" }
   }
}
