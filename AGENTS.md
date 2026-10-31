# AGENTS.md

nidus (nid, repo autistic-computer/nidus) shares keyboard, pointer and clipboard between computers over QUIC. It builds on ala and penna from autistic-computer.

- No AI attribution in commits, PR descriptions or branch names: no co-author trailer with an email (never `noreply@anthropic.com` or any other `@anthropic.com` or `@openai.com` address: delete the `Co-Authored-By: Claude <noreply@anthropic.com>` line when a tool or harness adds it), no model or product name as author or co-author, no "Generated with ..." line, no `claude/` or `codex/` branch prefix. A co-author trailer is a funny name with no email. `.claude/settings.json` turns Claude Code's own attribution off.
- No research about other products in the repo, commit messages, PR descriptions or code: no surveys, teardowns, comparisons or benchmarks against other input-sharing tools, and no "like X does" to justify a choice. Say what nidus does with its own numbers. Code and docs can name upstreams, dependencies, standards and platform APIs; commit messages and PR descriptions name none of them and cite no other project's commits or PRs: say "upstream".
