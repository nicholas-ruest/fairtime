# fairtime

`fairtime` plans a rotating UTC meeting schedule without making the same person
absorb every early morning or late evening. It reads each participant's UTC
offset and preferred local window, then greedily minimizes the maximum
accumulated out-of-hours burden, the burden spread, and finally total burden.

The result is deterministic and inspectable. It works offline and can emit text
for humans or JSON for calendars and automation.

## Install

```console
cargo install --git https://github.com/nicholas-ruest/fairtime
```

Or clone the repository and run it directly with stable Rust:

```console
cargo run --release -- examples/global-team.toml
```

## Configure

```toml
title = "Global design review"
occurrences = 4
step_minutes = 30
rotate = true
maximum_outside_minutes = 240

[[participants]]
name = "Vancouver"
utc_offset = "-08:00"
preferred_start = "09:00"
preferred_end = "17:00"

[[participants]]
name = "Bengaluru"
utc_offset = "+05:30"
preferred_start = "09:00"
preferred_end = "18:00"
weight = 2
```

Preferred-window edges are inclusive. Windows may cross midnight. A `weight`
greater than one increases the scheduling cost of burdening that participant.
With `rotate = true`, a UTC slot is used at most once in the generated plan.
`maximum_outside_minutes` is a hard guardrail: no participant may be scheduled
farther than that many minutes from their preferred window. The planner fails
with an explanation if no slot can satisfy it.

## Use

```console
fairtime examples/global-team.toml
fairtime examples/global-team.toml --format json
```

Example text output:

```text
Global design review

#1  14:00 UTC  |  Vancouver 06:00 (+180m)  |  Toronto 09:00 (preferred)  |  Berlin 15:00 (preferred)  |  Bengaluru 19:30 (+90m)
```

`outside_preference_minutes` is circular distance to the nearest edge of a
participant's preferred window. The planner picks each occurrence by comparing
projected weighted burden in this order:

1. lowest maximum participant burden;
2. smallest gap between the most and least burdened participants;
3. lowest total burden;
4. lowest immediate burden, then earliest UTC slot for deterministic ties.

This is a transparent greedy planner, not a claim of globally optimal
multi-week scheduling.

## Verify

```console
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Limitations

- Offsets are fixed strings, so daylight-saving changes must be reflected in a
  new input file.
- The schedule contains time-of-day slots, not calendar dates or invitations.
- Rotation uses a deterministic fairness heuristic rather than exhaustive
  optimization.

## License

MIT
