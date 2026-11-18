# Dynamic islands: how the real ones work (research, 2026-11-16)

Read from source (p3dro `modules/ii/dynamicIsland`, ~24k lines QML; DMS `Modules/DIsland`)
and Apple's Human Interface Guidelines (Live Activities). Goal: kestrel's island as one
interactive system for everything that pops up, adapted to a desktop (productivity, no lag).

## Apple (iPhone)

- Three presentations: **compact** (one activity: a leading and a trailing part around the
  sensor), **minimal** (two activities: one attached, one detached as a small circle or oval),
  **expanded** (touch and hold; also shown briefly for alerts). Expanded is an enlarged compact:
  same elements, same places.
- Tap opens the app; hold expands. As narrow as possible, no wasted space; black opaque surface.
- Alerts expand on their own for a moment, then fall back to compact.

## p3dro (ii-p3drovfx), the most complete open one

- **Sources** (`core/sources`, ~45): each event is a *transient* (an announcement: trigger +
  TTL) or a *continuous* state (bound to a service). Deriving events from state once, in the
  base class, is what fixed its worst bugs (a re-read clipboard is not a new copy).
- **Registry** (`IslandRegistry.qml`): one descriptor per activity: tier, priority, preferred
  side, can it detach, settle time, TTL, compact / orb / expanded sizes. Settings are generated
  from it.
- **Tiers** (`IslandLayout.js`, pure functions, unit-tested): interrupt (OSD, search, a request
  for approval) > transient (workspace, keyboard layout, clipboard) > live (transfer, recording,
  agent) > ambient (media) > idle (clock). Then the activity's own priority (a call beats a
  notification), then recency.
- **Slots**: a centre and up to two side bubbles. A new activity arrives in the centre, then
  after `settleMs` detaches to its side ("one object splitting"), handing the centre back to the
  clock. Interrupts and announcements never go to a side. An expanded activity is pinned (never
  moved under the pointer).
- **Notifications**: tier interrupt, TTL 4.5 s, settle 4 s; compact = title over body (or one
  line); the pointer on it stops the timers (being read), leaving counts as read after the
  collapse grace.
- **OSD**: interrupt, TTL 1.5 s, never detaches ("a slider belongs where the eye already is").
- **Hover**: expanded face after 600 ms (or hold-to-reveal, or click to expand); folds 450 ms
  after leaving (one grace for island and bubbles; 1500 ms "read as stuck").
- Expanding the clock opens a dashboard of quick tiles.

## DMS (DIsland)

Media, notifications and system sources (volume, brightness, caps lock, power profile, DND,
mic mute, idle inhibitor), interactive in place ("click the icon to mute").

## Other Hyprland islands (GitHub)

Common features: media with spectrum, notifications with inline reply, timers/alarms,
privacy (mic/camera) dots, launcher or clipboard from the island, CPU/RAM graphs, agent
status with Allow/Deny (Claude Code hooks).
