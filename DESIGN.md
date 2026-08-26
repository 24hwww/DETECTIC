# DETECTIC — DESIGN SYSTEM & DASHBOARD

## `DESIGN.md`

> **Project:** DETECTIC
> **Purpose:** Wi-Fi / RF intelligence and network telemetry dashboard
> **Design reference:** Shadcn Space Dashboard
> **Primary UI stack:** React + Vite + shadcn/ui + Tailwind CSS
> **UI philosophy:** Dense, professional, operational, data-first
> **Status:** Design specification
> **Last updated:** 2026-08-26

---

# 1. Design Objective

DETECTIC is not an administration dashboard.

It is an **observability and RF intelligence interface**.

The UI must allow an operator to answer, immediately:

1. What is happening now?
2. Which devices are connected?
3. Which devices disappeared?
4. Which devices were recently observed?
5. Which APs are nearby?
6. What RF environment was detected?
7. How strong is each signal?
8. How long has a device/AP been observed?
9. When was it last seen?
10. What changed recently?

The interface must prioritize **operational information over decoration**.

The Shadcn Space dashboard is the visual foundation: sidebar + topbar + KPI cards + charts + tables + widgets + responsive content grid. Shadcn Space explicitly provides these dashboard primitives as reusable blocks.

DETECTIC adapts that foundation to telemetry rather than business analytics.

---

# 2. Core Design Principles

## 2.1 Data first

Every visual element must communicate information.

Avoid:

* decorative gradients without meaning
* unnecessary illustrations
* excessive animations
* oversized empty cards
* fake metrics
* generic SaaS copy
* business/e-commerce terminology

Prefer:

* compact information density
* timestamps
* status indicators
* signal strength
* trend indicators
* event counts
* historical context
* expandable details

---

## 2.2 Real-time by default

DETECTIC is an event-driven monitoring system.

The dashboard should visually communicate whether data is:

* LIVE
* CONNECTED
* DEGRADED
* STALE
* OFFLINE

The user should never have to guess whether a value is current.

Global realtime indicator:

```text
● LIVE
```

Possible states:

```text
● LIVE
● SYNCING
● DEGRADED
● STALE
● OFFLINE
```

Do not use color alone to communicate these states.

Every status must have:

* icon
* text
* semantic color
* optional tooltip

---

# 3. Visual Language

## 3.1 General aesthetic

Target aesthetic:

> Modern network operations center + premium Shadcn dashboard.

Characteristics:

* neutral background
* strong typography
* subtle borders
* restrained shadows
* rounded cards
* compact spacing
* monochrome icons
* semantic accent colors
* dark mode first-class
* high information density

The visual language should feel closer to:

```text
Network Operations Center
+
Modern SaaS Analytics
+
RF Monitoring
```

and not:

```text
Marketing website
```

---

# 4. Color System

Use semantic colors rather than hardcoded colors inside components.

Define tokens through CSS variables.

## 4.1 Base

```text
--background
--foreground

--card
--card-foreground

--popover
--popover-foreground

--muted
--muted-foreground

--border
--input
--ring
```

Use the standard shadcn/ui token model.

---

## 4.2 DETECTIC semantic colors

Define:

```text
--detectic-connected
--detectic-disconnected
--detectic-warning
--detectic-danger
--detectic-info
--detectic-observed
--detectic-stale
```

Semantic meaning:

| State        | Meaning                                              |
| ------------ | ---------------------------------------------------- |
| Connected    | Device currently associated                          |
| Disconnected | Device no longer observed as connected               |
| Observed     | Device/AP detected by RF/environment observation     |
| Warning      | Anomaly or degraded state                            |
| Danger       | Critical failure                                     |
| Info         | Informational event                                  |
| Stale        | Data has not been refreshed within expected interval |

Do not infer status from color.

---

# 5. Typography

Use the default Shadcn/Tailwind typography hierarchy.

Recommended:

```text
Page title:
text-2xl / font-semibold

Section title:
text-lg / font-semibold

Card title:
text-sm / font-medium

Primary metric:
text-2xl or text-3xl / font-semibold

Secondary metric:
text-sm / text-muted-foreground

Metadata:
text-xs / text-muted-foreground

Timestamp:
text-xs / tabular-nums
```

Use `font-variant-numeric: tabular-nums` for:

* timestamps
* RSSI
* RCPI
* device counts
* durations
* percentages
* event counts

This prevents numerical values from visually jumping.

---

# 6. Layout Architecture

The application uses a standard dashboard shell.

```text
┌──────────────────────────────────────────────────────────────┐
│ Sidebar │ Topbar                                             │
│         ├─────────────────────────────────────────────────────┤
│         │                                                     │
│         │ Page                                                │
│         │                                                     │
│         │ Content grid                                        │
│         │                                                     │
│         │                                                     │
└──────────────────────────────────────────────────────────────┘
```

Reference architecture:

```text
AppShell
 ├── Sidebar
 │    ├── Logo
 │    ├── Main navigation
 │    ├── Monitoring
 │    ├── Intelligence
 │    ├── Network
 │    └── System
 │
 ├── Topbar
 │    ├── Breadcrumb
 │    ├── Search
 │    ├── Realtime status
 │    ├── Notifications
 │    └── User/system menu
 │
 └── MainContent
      ├── PageHeader
      └── Content
```

Shadcn Space's dashboard shell follows this general sidebar/header/content architecture and is intended to be reused as a full dashboard layout.

---

# 7. Sidebar

## 7.1 Navigation

The sidebar should contain:

```text
DETECTIC

Overview

MONITORING
  Live Monitor
  Devices
  Access Points
  RF Environment

INTELLIGENCE
  Sessions
  Events
  History
  Map

NETWORK
  Sensor
  Router
  Connectivity

SYSTEM
  Reports
  Settings
```

The navigation must remain compact.

---

## 7.2 Active item

Active navigation item:

```text
[icon] Overview
```

Use:

* subtle background
* foreground contrast
* optional left indicator

Do not use oversized colored pills.

---

## 7.3 Collapsed sidebar

Desktop supports:

```text
Expanded
Collapsed
```

Collapsed mode shows only icons.

Every icon must have a tooltip.

---

# 8. Topbar

Topbar structure:

```text
[☰] DETECTIC / Overview

                         Search
                         ● LIVE
                         🔔
                         Avatar
```

Elements:

### Breadcrumb

Example:

```text
DETECTIC / Devices / Device Details
```

### Search

Global search should eventually support:

* MAC
* hostname
* IP
* device ID
* AP BSSID
* event ID

Example:

```text
Search devices, APs, events...
```

### Realtime indicator

Always visible.

Example:

```text
● LIVE
```

Clicking it opens:

```text
Realtime connection
Transport: WebSocket/SSE
Last event: 2s ago
Latency: 48ms
Events received: 12,481
```

---

# 9. Overview Dashboard

The main page is the operational command center.

## 9.1 Page header

```text
Overview

Real-time network and RF intelligence

[Last 24h ▼] [Refresh]
```

---

# 10. KPI Row

Use compact Shadcn-style statistics cards.

Recommended KPIs:

```text
Connected Devices
Observed Devices
Nearby APs
Active Sessions
```

Example:

```text
┌────────────────────┐
│ Connected Devices  │
│                    │
│ 12                 │
│ +2 today            │
└────────────────────┘
```

Additional cards can include:

```text
Events / hour
New devices
Disconnected devices
RF observations
```

Do not display a KPI unless the backend provides a trustworthy value.

---

# 11. Live Network State

Primary visualization:

```text
┌──────────────────────────────────────────────────────────────┐
│ Live Network                                                 │
│                                                              │
│ Connected       Observed        APs         Events            │
│    12              8             17          241              │
│                                                              │
│                 [network visualization]                      │
└──────────────────────────────────────────────────────────────┘
```

The visualization must distinguish:

```text
Router
  │
  ├── Connected devices
  │
  ├── Sessions
  │
  └── RF observations
```

---

# 12. Live Events Feed

The dashboard must include a realtime event stream.

Example:

```text
LIVE EVENTS

● Device connected
  2C:54:91:XX:XX:XX
  12:41:32

● Device disconnected
  8A:21:XX:XX:XX:XX
  12:40:58

● AP observed
  TP-Link-XXXX
  channel 36 · -61 dBm
  12:40:41
```

Events should enter from the top.

Animation:

* subtle
* short
* no continuous motion

Do not animate the entire dashboard.

---

# 13. Devices Page

Route:

```text
/devices
```

Primary component:

**DataTable**

Columns:

```text
Status
Device
MAC
IP
Hostname
Signal
Band
First Seen
Last Seen
Duration
```

Example:

```text
● Connected
iPhone
AA:BB:CC:DD:EE:FF
192.168.0.15
iphone
-48 dBm
5 GHz
09:31
12:41
3h 10m
```

---

# 14. Device Status Model

The UI must distinguish:

```text
CONNECTED
DISCONNECTED
OBSERVED
STALE
UNKNOWN
```

Important:

`UNKNOWN` must not automatically become `CONNECTED`.

Status must come from the backend's canonical state.

This prevents the UI from reproducing historical data classification bugs.

---

# 15. Device Details

Route:

```text
/devices/:id
```

Header:

```text
Device

iPhone
AA:BB:CC:DD:EE:FF

● Connected
```

Sections:

```text
Current State
Signal
Session
History
Events
RF Correlation
```

---

# 16. Device Overview Card

```text
Current State

Status       Connected
Signal       -48 dBm
Band         5 GHz
AP           DETECTIC
First Seen   09:31
Last Seen    12:41
Duration     3h 10m
```

---

# 17. Signal Chart

Display signal history.

Preferred chart:

```text
Line chart
```

X-axis:

```text
time
```

Y-axis:

```text
signal strength
```

Do not invert the numeric value visually without explicitly labeling it.

If using RSSI:

```text
-30 dBm  = stronger
-80 dBm  = weaker
```

The chart should make that relationship intuitive.

---

# 18. Sessions

Session visualization:

```text
Session History

09:31 ━━━━━━━━━━━━━━━━━━━━━ 12:41
13:02 ━━━━━━━━ 13:48
14:11 ━━━━━━━━━
```

Each session should expose:

```text
Started
Ended
Duration
Signal range
AP
Band
```

---

# 19. Access Points Page

Route:

```text
/access-points
```

Purpose:

Display APs detected through the RF/environment observation pipeline.

Columns:

```text
AP
BSSID
SSID
Channel
Band
Signal
First Seen
Last Seen
Duration
Status
```

---

# 20. AP State

An AP is not necessarily a DETECTIC-connected network.

Use explicit states:

```text
DETECTED
ACTIVE
STALE
DISAPPEARED
```

Do not call an AP "online" unless the backend has enough evidence to support that interpretation.

Prefer:

```text
Last observed
```

over:

```text
Online since
```

unless a continuous observation session actually exists.

---

# 21. RF Environment

Route:

```text
/rf
```

Purpose:

Display the surrounding RF environment.

Primary widgets:

```text
Detected APs
2.4 GHz
5 GHz
6 GHz
Channels
Signal distribution
Noise
Channel utilization
```

---

# 22. RF Channel Visualization

Example:

```text
2.4 GHz

1   ███████████
6   ███████████████
11  ████████
```

For 5 GHz:

```text
36  █████████
40  █████
44  ███████████
48  ██
```

Each bar should represent a measurable backend value.

---

# 23. RF Fingerprint

An RF fingerprint card may contain:

```text
BSSID
SSID
Band
Channel
Signal
Noise
Security
Vendor
First Seen
Last Seen
Observation Count
```

This creates a historical identity for an AP rather than treating each scan as a new object.

---

# 24. Map

Route:

```text
/map
```

Map purpose:

Visualize DETECTIC telemetry geographically.

The map must **not** imply precise device location when the system only has signal-strength estimates.

The map should display:

```text
Router / Sensor
Cities
Relevant geographic boundaries
AP observations
Optional device observations
```

For the current regional use case, the geographic focus is:

```text
Brazil
└── Santa Catarina
    └── Cities
```

Do not render every Brazilian city when the user is operating in a Santa Catarina-focused view.

---

# 25. Map Semantics

Map markers:

```text
Sensor
AP
Device
Observation
```

Marker appearance must communicate entity type.

Example:

```text
● Sensor
◆ AP
○ Device
```

Avoid using identical markers for different entity types.

---

# 26. Signal Distance

Distance estimation must be explicitly labeled as an estimate.

Never display:

```text
Device: 4.2 meters
```

as a fact when derived only from signal strength.

Instead:

```text
Estimated proximity
≈ 4–8 m

Confidence: Medium
```

The UI must expose:

```text
Estimate
Confidence
Method
```

when such data exists.

---

# 27. Analytics

Route:

```text
/analytics
```

Charts:

```text
Devices over time
Connections over time
Disconnections
AP observations
RF observations
Signal trends
Session duration
```

Use Shadcn Space-style chart cards.

Shadcn Space provides bar, donut, line and weekly chart blocks suitable as reusable foundations.

---

# 28. Time Windows

Standard selector:

```text
5m
15m
1h
6h
24h
7d
30d
Custom
```

Changing the time window should update all compatible widgets consistently.

---

# 29. Events

Route:

```text
/events
```

DataTable:

```text
Timestamp
Type
Entity
Status
Source
Details
```

Event types:

```text
DEVICE_CONNECTED
DEVICE_DISCONNECTED
DEVICE_OBSERVED
AP_OBSERVED
AP_DISAPPEARED
SESSION_STARTED
SESSION_ENDED
RF_SNAPSHOT
SENSOR_ONLINE
SENSOR_OFFLINE
```

The actual enum must be synchronized with the backend.

Never invent event types only for UI purposes.

---

# 30. Event Detail

Clicking an event opens a Sheet/Dialog.

Example:

```text
Event Details

DEVICE_CONNECTED

12:41:32

Device
AA:BB:CC:DD:EE:FF

Source
EX520

Signal
-48 dBm

Band
5 GHz

Session
abc123
```

---

# 31. Sensor Status

Route:

```text
/sensor
```

Display:

```text
Sensor Status

● Online

Last heartbeat
12:41:42

Transport
Connected

Events received
12,481

Last event
12:41:41
```

Additional:

```text
CPU
Memory
Uptime
Firmware
Sensor version
```

Only show metrics actually supplied by the sensor.

---

# 32. Router Status

Route:

```text
/router
```

Display:

```text
Router

TP-Link EX520

● Reachable

GTPR
Connected

Last poll
12:41:41

Associated devices
12

RF observations
17
```

The UI should distinguish:

```text
Router reachable
```

from:

```text
Realtime transport connected
```

These are different states.

---

# 33. Reports

Route:

```text
/reports
```

Reports should summarize:

```text
Period
Devices
Sessions
APs
RF environment
Events
Anomalies
```

Email reports should use the same semantic vocabulary as the dashboard.

Dashboard and email must not calculate different versions of:

* connected
* disconnected
* session
* duration
* last seen

The backend remains the source of truth.

---

# 34. Tables

Use Shadcn/TanStack-style data tables.

Required capabilities:

```text
Sorting
Filtering
Pagination
Column visibility
Row selection
Search
```

For large datasets:

```text
Server-side pagination
Server-side filtering
```

Do not load the entire event history into the browser.

---

# 35. Filters

Reusable filter bar:

```text
[Status ▼]
[Band ▼]
[Time ▼]
[Signal ▼]
[Source ▼]
[Search]
```

Filters should be URL-addressable where practical.

Example:

```text
/devices?status=connected&band=5ghz
```

---

# 36. Empty States

Never show an empty white/blank card.

Use:

```text
No devices detected

DETECTIC has not received device observations
for this period.

[Adjust time range]
```

Empty states must explain:

1. what is empty
2. why it may be empty
3. what the user can do

---

# 37. Loading States

Use skeletons for initial page loading.

Example:

```text
┌────────────────────┐
│ ████████████████   │
│ ███████            │
│                    │
│ █████████████      │
└────────────────────┘
```

Do not show a spinner for every individual metric.

---

# 38. Error States

Example:

```text
Unable to load devices

The dashboard could not retrieve the latest
device telemetry.

[Retry]
```

Technical errors should be available through an expandable detail section.

---

# 39. Stale Data

Every realtime-derived page must be capable of displaying stale state.

Example:

```text
● STALE

Last update
2m 14s ago
```

A stale dashboard must not continue visually presenting itself as live.

---

# 40. Realtime Event UX

When an event arrives:

1. Update the affected entity.
2. Update relevant KPI.
3. Update event feed.
4. Update charts when appropriate.
5. Update timestamp.
6. Do not reload the entire page.

Avoid:

```text
window.location.reload()
```

or full dashboard refetches for every event.

---

# 41. Animation

Animation should communicate state changes.

Allowed:

* event insertion
* status transition
* drawer opening
* dropdowns
* subtle chart transitions
* map marker transition

Avoid:

* infinite decorative animations
* bouncing cards
* excessive motion
* animated backgrounds

Respect:

```text
prefers-reduced-motion
```

---

# 42. Cards

Standard card anatomy:

```text
Card
 ├── Header
 │    ├── Title
 │    └── Action
 ├── Content
 └── Footer
```

Example:

```text
┌──────────────────────────────────────┐
│ Connected Devices              ⋮     │
│                                      │
│ 12                                   │
│                                      │
│ +2 in the last hour                  │
└──────────────────────────────────────┘
```

---

# 43. Responsive Design

Desktop:

```text
Sidebar + Topbar + Grid
```

Tablet:

```text
Collapsed Sidebar + Grid
```

Mobile:

```text
Topbar
Bottom/Sheet navigation
Single-column content
```

Grid behavior:

```text
Desktop:
4 / 3 / 2 columns depending on component

Tablet:
2 columns

Mobile:
1 column
```

Tables should become horizontally scrollable rather than destroying column information.

---

# 44. Dark Mode

Dark mode is a first-class mode.

Requirements:

* no hardcoded white backgrounds
* no hardcoded black text
* semantic CSS variables
* charts must remain readable
* map controls must remain readable
* status indicators must retain sufficient contrast

---

# 45. Accessibility

Minimum:

```text
WCAG-compatible contrast
Keyboard navigation
Visible focus
ARIA labels where needed
Tooltip for icon-only controls
Semantic HTML
Accessible dialogs/sheets
Accessible tables
```

Shadcn Space emphasizes accessible foundations and supports Radix/Base UI primitives for this purpose.

---

# 46. Component Architecture

Recommended structure:

```text
src/
├── components/
│   ├── ui/
│   ├── dashboard/
│   ├── devices/
│   ├── access-points/
│   ├── rf/
│   ├── events/
│   ├── map/
│   ├── sensor/
│   └── router/
│
├── layouts/
│   └── dashboard-layout.jsx
│
├── pages/
│
├── lib/
│
└── hooks/
```

Prefer `.js` / `.jsx` when the existing project convention uses JavaScript.

Do not migrate the project to TypeScript merely for the design system.

---

# 47. Shadcn Space Integration

Use Shadcn Space as a source of reusable UI structures rather than copying an entire unrelated dashboard.

Relevant building blocks:

```text
dashboard-shell
sidebar
topbar
statistics
charts
widgets
tables
dialogs
forms
empty states
```

Shadcn Space explicitly supports installing blocks through the shadcn CLI and copying their source into the project, preserving ownership of the resulting code.

Where appropriate, use the Shadcn Space registry workflow:

```bash
pnpm dlx shadcn@latest add @shadcn-space/...
```

or the registry URL mechanism documented by Shadcn Space.

Do not blindly install every available block.

Only install components that serve DETECTIC.

---

# 48. Recommended Initial Components

Priority 1:

```text
Dashboard Shell
Sidebar
Topbar
Statistics
Card
Badge
Button
Dropdown
Sheet
Dialog
Table
Chart
Tooltip
Skeleton
```

Priority 2:

```text
DataTable
Tabs
Date Range Picker
Command/Search
Progress
Separator
Scroll Area
```

Priority 3:

```text
Advanced charts
Timeline
Map controls
RF visualization
```

---

# 49. DETECTIC-Specific Components

Create dedicated components for domain semantics.

Examples:

```text
<DeviceStatusBadge />

<SignalBadge />

<SignalStrength />

<ConnectionDuration />

<LastSeen />

<RealtimeIndicator />

<EventTypeBadge />

<ApStatusBadge />

<SessionTimeline />

<RfChannelChart />

<DeviceSignalChart />

<LiveEventFeed />

<SensorHealth />

<RouterHealth />
```

These components should encapsulate DETECTIC semantics.

---

# 50. Status Badge Contract

Example:

```jsx
<DeviceStatusBadge status="connected" />
```

Possible values:

```text
connected
disconnected
observed
stale
unknown
```

The component controls:

* icon
* label
* semantic color
* accessibility label

The page should not duplicate status logic.

---

# 51. Signal Component Contract

Example:

```jsx
<SignalStrength
  value={-48}
  unit="dBm"
/>
```

Optional:

```jsx
<SignalStrength
  value={-48}
  unit="dBm"
  showQuality
/>
```

Do not convert signal to distance inside the UI.

---

# 52. Time Representation

Always store timestamps in UTC/backend canonical format.

Render according to user locale.

Display:

```text
12:41:32
```

For historical records:

```text
26 Aug 2026, 12:41
```

Relative form may be used:

```text
2 min ago
```

but should expose the exact timestamp on hover/click.

---

# 53. Duration Representation

Use human-readable duration:

```text
3h 14m
42m
18s
2d 4h
```

For tables, prefer compact notation.

For details, allow:

```text
3 hours, 14 minutes
```

---

# 54. Device Identity

Primary identity:

```text
canonical device ID
```

Secondary:

```text
MAC
hostname
IP
vendor
display name
```

The UI must not assume that:

```text
MAC = human-readable device identity
```

Vendor/model classification may be uncertain.

Use confidence when applicable:

```text
Apple iPhone
Confidence: High
```

---

# 55. Unknown Identity

If the system cannot classify a device:

```text
Unknown device
```

Do not manufacture:

```text
iPhone
Samsung
Laptop
```

based only on weak evidence.

---

# 56. AP Identity

AP identity should prioritize:

```text
BSSID
```

with:

```text
SSID
Vendor
Channel
Band
```

BSSID is the stable RF identity when available.

---

# 57. Data Provenance

Important telemetry should expose source when useful.

Example:

```text
Source
EX520 / GTPR
```

or:

```text
Source
RF Sensor
```

or:

```text
Source
Backend event
```

This is especially important when multiple observation mechanisms exist.

---

# 58. Confidence

When a value is inferred rather than directly observed, show confidence.

Example:

```text
Estimated proximity

8–15 m
Medium confidence
```

Possible:

```text
High
Medium
Low
Unknown
```

Never present inferred values as direct measurements.

---

# 59. Performance

The dashboard must remain responsive with:

```text
100+ devices
1,000+ events
10,000+ historical events
```

Use:

* pagination
* virtualization when necessary
* memoized expensive visualizations
* incremental realtime updates
* server-side filtering
* lazy loading for secondary pages

Do not render thousands of DOM nodes unnecessarily.

---

# 60. Realtime Architecture Boundary

Frontend should consume a normalized event stream.

Conceptually:

```text
EX520
  ↓
Sensor
  ↓
Transport
  ↓
Cloudflare
  ↓
Canonical Event
  ↓
Realtime Transport
  ↓
DETECTIC UI
```

The UI should not understand EX520-specific transport details.

Frontend receives canonical events.

Example:

```js
{
  type: "DEVICE_CONNECTED",
  timestamp: "...",
  entity: {
    id: "...",
    type: "device"
  },
  data: {
    signal: -48,
    band: "5ghz"
  }
}
```

---

# 61. Canonical State

Frontend state should distinguish:

```text
entity state
event history
session history
RF observations
connection state
```

Do not derive persistent state exclusively from the visual event feed.

The backend remains authoritative.

---

# 62. Dashboard Information Hierarchy

Every page should follow:

```text
1. Current state
2. Important metrics
3. Recent changes
4. Historical context
5. Detailed records
```

Example:

```text
DEVICE

Current status
      ↓
Signal
      ↓
Current session
      ↓
Recent events
      ↓
Signal history
      ↓
Session history
```

---

# 63. Do Not Build

The implementation must NOT introduce:

```text
❌ Generic CRM dashboard
❌ Sales terminology
❌ Fake revenue metrics
❌ Fake AI insights
❌ Fake device classifications
❌ Fake distance measurements
❌ Fake online status
❌ Decorative charts without backend data
❌ Hardcoded telemetry
```

The dashboard must represent actual DETECTIC telemetry.

---

# 64. Design Rule: Backend Truth

The UI must never redefine backend semantics.

If backend says:

```text
status = unknown
```

the UI displays:

```text
Unknown
```

It must not transform it into:

```text
Connected
```

because the entity appeared in a poll.

This rule is mandatory.

---

# 65. Design Rule: Observed ≠ Connected

DETECTIC has multiple observation mechanisms.

Therefore:

```text
Connected
```

means an actual association/current connection state.

While:

```text
Observed
```

means the system detected an entity.

These concepts must remain visually and semantically separate everywhere:

* cards
* tables
* map
* charts
* reports
* email
* notifications

---

# 66. Design Rule: Last Seen ≠ Connected Duration

A device can have:

```text
Last seen
```

without having:

```text
Current session duration
```

Only show duration when a valid session exists.

---

# 67. Design Rule: AP Observation ≠ AP Availability

For RF observations:

Prefer:

```text
Last observed
Observation duration
Observation count
```

over:

```text
Online
Uptime
```

unless the backend explicitly provides continuous availability semantics.

---

# 68. Design Tokens

Centralize:

```text
spacing
radius
typography
colors
shadows
transitions
breakpoints
```

Example:

```text
radius:
sm
md
lg
xl

spacing:
1
2
3
4
6
8
12
16
24
```

Avoid arbitrary per-component values.

---

# 69. Iconography

Use Lucide icons consistently.

Examples:

```text
Activity
Wifi
Router
Radio
Smartphone
Laptop
Server
Map
Clock
Signal
Database
Bell
Search
Settings
AlertTriangle
CircleCheck
CircleX
```

Do not mix unrelated icon libraries without a reason.

Shadcn Space dashboard blocks already use Lucide React among their dependencies in several dashboard components.

---

# 70. Final Dashboard

The primary DETECTIC screen should approximately follow:

```text
┌─────────────────────────────────────────────────────────────────┐
│ DETECTIC                         Search       ● LIVE   🔔       │
├──────────────┬──────────────────────────────────────────────────┤
│              │                                                  │
│ Overview     │ Overview                                         │
│              │ Real-time network and RF intelligence            │
│ Monitoring   │                                                  │
│  Live        │ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐    │
│  Devices     │ │Devices │ │Observed│ │APs     │ │Events  │    │
│  APs         │ │   12   │ │    8   │ │   17   │ │  241   │    │
│  RF          │ └────────┘ └────────┘ └────────┘ └────────┘    │
│              │                                                  │
│ Intelligence │ ┌──────────────────────┐ ┌───────────────────┐  │
│  Sessions    │ │                      │ │                   │  │
│  Events      │ │   LIVE NETWORK       │ │   LIVE EVENTS     │  │
│  History     │ │                      │ │                   │  │
│  Map         │ │                      │ │ ● Connected       │  │
│              │ │                      │ │ ● AP observed     │  │
│ Network      │ │                      │ │ ● Disconnected    │  │
│  Sensor      │ └──────────────────────┘ └───────────────────┘  │
│  Router      │                                                  │
│              │ ┌─────────────────────────────────────────────┐  │
│ System       │ │ Signal / Sessions / RF Analytics            │  │
│  Reports     │ │                                             │  │
│  Settings    │ │                 chart                       │  │
│              │ │                                             │  │
└───────────tes┴──────────────────────────────────────────────────┘
```

---

# 71. Implementation Priority

## Phase 1 — Shell

```text
[ ] Dashboard layout
[ ] Sidebar
[ ] Topbar
[ ] Dark mode
[ ] Responsive behavior
[ ] Realtime indicator
```

## Phase 2 — Overview

```text
[ ] KPI cards
[ ] Live event feed
[ ] Network state
[ ] Device summary
[ ] AP summary
```

## Phase 3 — Devices

```text
[ ] Device table
[ ] Filters
[ ] Device details
[ ] Signal chart
[ ] Sessions
```

## Phase 4 — AP/RF

```text
[ ] AP table
[ ] AP details
[ ] RF environment
[ ] Channel visualization
[ ] RF history
```

## Phase 5 — Intelligence

```text
[ ] Events
[ ] Analytics
[ ] Sessions
[ ] History
[ ] Map
```

## Phase 6 — Infrastructure

```text
[ ] Sensor status
[ ] Router status
[ ] Transport status
[ ] Diagnostics
```

## Phase 7 — Reports

```text
[ ] Report dashboard
[ ] Historical summaries
[ ] Email/dashboard semantic consistency
```

---

# 72. Definition of Done

The design is considered correctly implemented when:

```text
[ ] Dashboard visually follows the Shadcn Space design language
[ ] Sidebar and topbar are reusable
[ ] Dark mode works
[ ] Mobile layout works
[ ] All telemetry is backend-driven
[ ] Connected and Observed are distinct
[ ] Unknown is not converted to Connected
[ ] Last Seen is distinct from session duration
[ ] AP observation is distinct from AP availability
[ ] Realtime status is visible
[ ] Event feed updates incrementally
[ ] Tables support filtering
[ ] Device details expose historical context
[ ] RF data has dedicated visualization
[ ] Map does not imply false precision
[ ] Estimated distance exposes confidence
[ ] Empty states are meaningful
[ ] Loading states use skeletons
[ ] Errors provide recovery
[ ] Accessibility is preserved
[ ] No fake metrics exist
[ ] No hardcoded production telemetry exists
```

---

# 73. Design North Star

The final product should feel like:

> **“A professional RF/network intelligence console built with Shadcn.”**

Not:

> “A Shadcn admin template with router data inserted into it.”

The design system exists to make DETECTIC's telemetry understandable.

The visual hierarchy must always answer:

```text
WHAT IS HAPPENING?
        ↓
WHAT CHANGED?
        ↓
WHAT WAS DETECTED?
        ↓
HOW LONG?
        ↓
HOW STRONG?
        ↓
WHEN?
        ↓
WHY / SOURCE?
```

That is the central design principle of DETECTIC.
