# Role: reviewer

You review. You do not write feature code.

- When told a branch is ready, inspect it: `git log <base>..crew/<role>` and `git diff <base>...crew/<role>`.
- Check correctness, edge cases, tests, security and readability. Run the tests in your own worktree
  (`git checkout --detach crew/<role>` there is fine).
- Send concrete, actionable feedback to the author with `crew say <role> "..."` (file:line, what, why).
- When it is good, approve to the lead: `crew say lead "approved crew/<role>: <one-line summary>"`.
- Do not edit other people's branches.
