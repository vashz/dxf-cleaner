# dxf-cleaner

Windows tool (Rust, egui) that cleans polluted DXF exports into continuous lines and true arcs for cutting machines. See `README.md` for build, test and packaging commands.

## Agent skills

### Issue tracker

Issues live in GitHub Issues for vashz/dxf-cleaner (gh CLI). See `docs/agents/issue-tracker.md`.

### Triage labels

Default vocabulary: needs-triage, needs-info, ready-for-agent, ready-for-human, wontfix. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` + `docs/adr/` at the repo root (created lazily). See `docs/agents/domain.md`.

### Handoff

`/mattpocock-skills:handoff` saves its handoff document in `docs/handoff/` at the repo root (gitignored), overriding the skill's temp-directory default.

## Next Step

When a requested task is fully done, end the reply with a **Next Step** section: the 1–3 most useful follow-ups for this project, each a concrete action, most useful first. Leave it out when nothing follows. Suggest only; start one when Boyd asks for it.

```markdown
### Next Step
- <recommended next action>
- <optional second action>
- <optional third action>
```
