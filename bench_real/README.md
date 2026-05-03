# Real Bench Prompts

These are the real benchmark prompts for `notclaude-coinflip`.

Run each prompt from the repository root. The prompt artifacts should be written
under `bench_real/output/` so the results are easy to inspect in this project.

Suggested manual loop:

```bash
notclaude-coinflip run "$(cat bench_real/simple_prompt.txt)"
notclaude-coinflip run "$(cat bench_real/complex_prompt.txt)"
notclaude-coinflip run "$(cat bench_real/more_complex_prompt.txt)"
```

For the 3x benchmark, run each prompt three times and keep the terminal output,
elapsed time, and final evaluator judgment alongside the files created in
`bench_real/output/`.
