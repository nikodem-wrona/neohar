---
title: "Events"
description: "The 49 kinds of events in the event outbox."
sidebar_position: 4
---

:::info[Generated page]

This page is generated from [`crates/nh-proto/tests/snapshots/api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap) by `website/scripts/gen-api.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The core appends an event in the same SQLite transaction as the state change that it describes (the transactional outbox). A client reads the log with [`events.subscribe`](/docs/reference/api/methods/events#events-subscribe) from a cursor and gets each event in an [`events.event`](#notification-events-event) notification. When the cursor is older than the retained log, the client gets [`events.truncated`](#notification-events-truncated). `v` is the version of the payload of a kind. See [API concepts](/docs/reference/api/concepts).

| Event | v | Payload |
|---|---|---|
| [`agent.detected`](#agent-detected) | 1 | [`AgentDetected`](/docs/reference/api/types#agentdetected) |
| [`agent.identified`](#agent-identified) | 1 | [`AgentIdentified`](/docs/reference/api/types#agentidentified) |
| [`agent.state_changed`](#agent-state-changed) | 1 | [`AgentStateChanged`](/docs/reference/api/types#agentstatechanged) |
| [`agent.harness_version_unvalidated`](#agent-harness-version-unvalidated) | 1 | [`AgentHarnessVersionUnvalidated`](/docs/reference/api/types#agentharnessversionunvalidated) |
| [`agent.drift_detected`](#agent-drift-detected) | 1 | [`AgentDriftDetected`](/docs/reference/api/types#agentdriftdetected) |
| [`adapter.disagreement`](#adapter-disagreement) | 1 | [`AdapterDisagreement`](/docs/reference/api/types#adapterdisagreement) |
| [`harness.manifest_loaded`](#harness-manifest-loaded) | 1 | [`HarnessManifestLoaded`](/docs/reference/api/types#harnessmanifestloaded) |
| [`interaction.opened`](#interaction-opened) | 1 | [`InteractionOpened`](/docs/reference/api/types#interactionopened) |
| [`interaction.decided`](#interaction-decided) | 1 | [`InteractionDecided`](/docs/reference/api/types#interactiondecided) |
| [`interaction.delivery_started`](#interaction-delivery-started) | 1 | [`InteractionDelivery`](/docs/reference/api/types#interactiondelivery) |
| [`interaction.delivered`](#interaction-delivered) | 1 | [`InteractionDelivery`](/docs/reference/api/types#interactiondelivery) |
| [`interaction.delivery_unknown`](#interaction-delivery-unknown) | 1 | [`InteractionDelivery`](/docs/reference/api/types#interactiondelivery) |
| [`interaction.delivery_failed`](#interaction-delivery-failed) | 1 | [`InteractionDeliveryFailed`](/docs/reference/api/types#interactiondeliveryfailed) |
| [`interaction.resolved_elsewhere`](#interaction-resolved-elsewhere) | 1 | [`InteractionEnded`](/docs/reference/api/types#interactionended) |
| [`interaction.dismissed`](#interaction-dismissed) | 1 | [`InteractionEnded`](/docs/reference/api/types#interactionended) |
| [`interaction.cancelled`](#interaction-cancelled) | 1 | [`InteractionCancelled`](/docs/reference/api/types#interactioncancelled) |
| [`pane.input_unconfirmed`](#pane-input-unconfirmed) | 1 | [`PaneInputUnconfirmed`](/docs/reference/api/types#paneinputunconfirmed) |
| [`pane.created`](#pane-created) | 1 | [`PaneEvent`](/docs/reference/api/types#paneevent) |
| [`pane.closed`](#pane-closed) | 1 | [`PaneEvent`](/docs/reference/api/types#paneevent) |
| [`pane.moved`](#pane-moved) | 1 | [`PaneEvent`](/docs/reference/api/types#paneevent) |
| [`pane.floated`](#pane-floated) | 1 | [`PaneEvent`](/docs/reference/api/types#paneevent) |
| [`pane.unfloated`](#pane-unfloated) | 1 | [`PaneEvent`](/docs/reference/api/types#paneevent) |
| [`pane.restarted`](#pane-restarted) | 1 | [`PaneEvent`](/docs/reference/api/types#paneevent) |
| [`pane.exited`](#pane-exited) | 1 | [`PaneExited`](/docs/reference/api/types#paneexited) |
| [`pane.bell`](#pane-bell) | 1 | [`PaneBell`](/docs/reference/api/types#panebell) |
| [`workspace.created`](#workspace-created) | 1 | [`WorkspaceEvent`](/docs/reference/api/types#workspaceevent) |
| [`workspace.renamed`](#workspace-renamed) | 1 | [`WorkspaceEvent`](/docs/reference/api/types#workspaceevent) |
| [`workspace.moved`](#workspace-moved) | 1 | [`WorkspaceEvent`](/docs/reference/api/types#workspaceevent) |
| [`workspace.closed`](#workspace-closed) | 1 | [`WorkspaceEvent`](/docs/reference/api/types#workspaceevent) |
| [`group.created`](#group-created) | 1 | [`GroupEvent`](/docs/reference/api/types#groupevent) |
| [`group.renamed`](#group-renamed) | 1 | [`GroupEvent`](/docs/reference/api/types#groupevent) |
| [`group.moved`](#group-moved) | 1 | [`GroupEvent`](/docs/reference/api/types#groupevent) |
| [`group.closed`](#group-closed) | 1 | [`GroupEvent`](/docs/reference/api/types#groupevent) |
| [`tab.created`](#tab-created) | 1 | [`TabEvent`](/docs/reference/api/types#tabevent) |
| [`tab.renamed`](#tab-renamed) | 1 | [`TabEvent`](/docs/reference/api/types#tabevent) |
| [`tab.moved`](#tab-moved) | 1 | [`TabEvent`](/docs/reference/api/types#tabevent) |
| [`tab.closed`](#tab-closed) | 1 | [`TabEvent`](/docs/reference/api/types#tabevent) |
| [`tab.selected`](#tab-selected) | 1 | [`TabEvent`](/docs/reference/api/types#tabevent) |
| [`layout.changed`](#layout-changed) | 1 | [`LayoutChanged`](/docs/reference/api/types#layoutchanged) |
| [`window.opened`](#window-opened) | 1 | [`WindowEvent`](/docs/reference/api/types#windowevent) |
| [`window.closed`](#window-closed) | 1 | [`WindowEvent`](/docs/reference/api/types#windowevent) |
| [`phone.paired`](#phone-paired) | 1 | [`PhoneEvent`](/docs/reference/api/types#phoneevent) |
| [`phone.renamed`](#phone-renamed) | 1 | [`PhoneEvent`](/docs/reference/api/types#phoneevent) |
| [`phone.removed`](#phone-removed) | 1 | [`PhoneEvent`](/docs/reference/api/types#phoneevent) |
| [`phone.pushed`](#phone-pushed) | 1 | [`PhonePushed`](/docs/reference/api/types#phonepushed) |
| [`phone.push_failed`](#phone-push-failed) | 1 | [`PhonePushFailed`](/docs/reference/api/types#phonepushfailed) |
| [`phone.replied`](#phone-replied) | 1 | [`PhoneReplied`](/docs/reference/api/types#phonereplied) |
| [`policy.added`](#policy-added) | 1 | [`PolicyEvent`](/docs/reference/api/types#policyevent) |
| [`policy.removed`](#policy-removed) | 1 | [`PolicyEvent`](/docs/reference/api/types#policyevent) |

## agent events {#group-agent}

### `agent.detected` {#agent-detected}

**Version:** 1

[`AgentDetected`](/docs/reference/api/types#agentdetected)

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `via` | string | yes | How the core found it: `launch`, `hook`, `process`, `screen`. |

### `agent.identified` {#agent-identified}

**Version:** 1

[`AgentIdentified`](/docs/reference/api/types#agentidentified)

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `session_id` | string | yes |  |

### `agent.state_changed` {#agent-state-changed}

**Version:** 1

[`AgentStateChanged`](/docs/reference/api/types#agentstatechanged)

| Field | Type | Required | Description |
|---|---|---|---|
| `confidence` | number (float) | yes |  |
| `facet` | [`Facet`](/docs/reference/api/types#facet) | yes |  |
| `from` | any | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `source` | [`Source`](/docs/reference/api/types#source) | yes |  |
| `to` | any | yes |  |

### `agent.harness_version_unvalidated` {#agent-harness-version-unvalidated}

**Version:** 1

[`AgentHarnessVersionUnvalidated`](/docs/reference/api/types#agentharnessversionunvalidated)

| Field | Type | Required | Description |
|---|---|---|---|
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `version` | string | yes |  |

### `agent.drift_detected` {#agent-drift-detected}

**Version:** 1

[`AgentDriftDetected`](/docs/reference/api/types#agentdriftdetected)

| Field | Type | Required | Description |
|---|---|---|---|
| `bad_signal_rate` | number (float) | yes |  |
| `harness` | [`Harness`](/docs/reference/api/types#harness) | yes |  |
| `observations` | integer (uint32) | yes |  |
| `version` | string | yes |  |

## adapter events {#group-adapter}

### `adapter.disagreement` {#adapter-disagreement}

**Version:** 1

[`AdapterDisagreement`](/docs/reference/api/types#adapterdisagreement)

| Field | Type | Required | Description |
|---|---|---|---|
| `facet` | [`Facet`](/docs/reference/api/types#facet) | yes |  |
| `other` | any | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |
| `structured` | any | yes |  |

## harness events {#group-harness}

### `harness.manifest_loaded` {#harness-manifest-loaded}

**Version:** 1

[`HarnessManifestLoaded`](/docs/reference/api/types#harnessmanifestloaded)

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |
| `serial` | integer (uint64) | yes |  |
| `source` | string | yes |  |
| `version` | string | yes |  |

## interaction events {#group-interaction}

### `interaction.opened` {#interaction-opened}

**Version:** 1

[`InteractionOpened`](/docs/reference/api/types#interactionopened)

| Field | Type | Required | Description |
|---|---|---|---|
| `by_policy` | boolean |  | Policy decided it at once. Default: `false`. |
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `kind` | [`InteractionKind`](/docs/reference/api/types#interactionkind) | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) \| `null` |  |  |
| `risk` | [`Risk`](/docs/reference/api/types#risk) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) \| `null` |  |  |

### `interaction.decided` {#interaction-decided}

**Version:** 1

[`InteractionDecided`](/docs/reference/api/types#interactiondecided)

| Field | Type | Required | Description |
|---|---|---|---|
| `by` | string | yes |  |
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `rev` | integer (uint32) | yes |  |
| `via` | [`Via`](/docs/reference/api/types#via) | yes |  |

### `interaction.delivery_started` {#interaction-delivery-started}

**Version:** 1

[`InteractionDelivery`](/docs/reference/api/types#interactiondelivery)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `rev` | integer (uint32) | yes |  |

### `interaction.delivered` {#interaction-delivered}

**Version:** 1

[`InteractionDelivery`](/docs/reference/api/types#interactiondelivery)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `rev` | integer (uint32) | yes |  |

### `interaction.delivery_unknown` {#interaction-delivery-unknown}

**Version:** 1

[`InteractionDelivery`](/docs/reference/api/types#interactiondelivery)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `rev` | integer (uint32) | yes |  |

### `interaction.delivery_failed` {#interaction-delivery-failed}

**Version:** 1

[`InteractionDeliveryFailed`](/docs/reference/api/types#interactiondeliveryfailed)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `reason` | string | yes |  |
| `rev` | integer (uint32) | yes |  |

### `interaction.resolved_elsewhere` {#interaction-resolved-elsewhere}

**Version:** 1

[`InteractionEnded`](/docs/reference/api/types#interactionended)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |

### `interaction.dismissed` {#interaction-dismissed}

**Version:** 1

[`InteractionEnded`](/docs/reference/api/types#interactionended)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |

### `interaction.cancelled` {#interaction-cancelled}

**Version:** 1

[`InteractionCancelled`](/docs/reference/api/types#interactioncancelled)

| Field | Type | Required | Description |
|---|---|---|---|
| `interaction` | [`InteractionId`](/docs/reference/api/types#interactionid) | yes |  |
| `reason` | string | yes | For example `process_exited`. |

## pane events {#group-pane}

### `pane.input_unconfirmed` {#pane-input-unconfirmed}

**Version:** 1

[`PaneInputUnconfirmed`](/docs/reference/api/types#paneinputunconfirmed)

| Field | Type | Required | Description |
|---|---|---|---|
| `input_id` | integer (uint64) | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `reason` | [`UnconfirmedReason`](/docs/reference/api/types#unconfirmedreason) | yes |  |

### `pane.created` {#pane-created}

**Version:** 1

[`PaneEvent`](/docs/reference/api/types#paneevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `pane.closed` {#pane-closed}

**Version:** 1

[`PaneEvent`](/docs/reference/api/types#paneevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `pane.moved` {#pane-moved}

**Version:** 1

[`PaneEvent`](/docs/reference/api/types#paneevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `pane.floated` {#pane-floated}

**Version:** 1

[`PaneEvent`](/docs/reference/api/types#paneevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `pane.unfloated` {#pane-unfloated}

**Version:** 1

[`PaneEvent`](/docs/reference/api/types#paneevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `pane.restarted` {#pane-restarted}

**Version:** 1

[`PaneEvent`](/docs/reference/api/types#paneevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `pane.exited` {#pane-exited}

**Version:** 1

[`PaneExited`](/docs/reference/api/types#paneexited)

| Field | Type | Required | Description |
|---|---|---|---|
| `code` | integer (int32) \| `null` |  |  |
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |

### `pane.bell` {#pane-bell}

**Version:** 1

[`PaneBell`](/docs/reference/api/types#panebell)

| Field | Type | Required | Description |
|---|---|---|---|
| `pane` | [`PaneId`](/docs/reference/api/types#paneid) | yes |  |

## workspace events {#group-workspace}

### `workspace.created` {#workspace-created}

**Version:** 1

[`WorkspaceEvent`](/docs/reference/api/types#workspaceevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `name` | string \| `null` |  |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `workspace.renamed` {#workspace-renamed}

**Version:** 1

[`WorkspaceEvent`](/docs/reference/api/types#workspaceevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `name` | string \| `null` |  |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `workspace.moved` {#workspace-moved}

**Version:** 1

[`WorkspaceEvent`](/docs/reference/api/types#workspaceevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `name` | string \| `null` |  |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `workspace.closed` {#workspace-closed}

**Version:** 1

[`WorkspaceEvent`](/docs/reference/api/types#workspaceevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `name` | string \| `null` |  |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

## group events {#group-group}

### `group.created` {#group-created}

**Version:** 1

[`GroupEvent`](/docs/reference/api/types#groupevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`GroupId`](/docs/reference/api/types#groupid) | yes |  |
| `name` | string \| `null` |  |  |

### `group.renamed` {#group-renamed}

**Version:** 1

[`GroupEvent`](/docs/reference/api/types#groupevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`GroupId`](/docs/reference/api/types#groupid) | yes |  |
| `name` | string \| `null` |  |  |

### `group.moved` {#group-moved}

**Version:** 1

[`GroupEvent`](/docs/reference/api/types#groupevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`GroupId`](/docs/reference/api/types#groupid) | yes |  |
| `name` | string \| `null` |  |  |

### `group.closed` {#group-closed}

**Version:** 1

[`GroupEvent`](/docs/reference/api/types#groupevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `group` | [`GroupId`](/docs/reference/api/types#groupid) | yes |  |
| `name` | string \| `null` |  |  |

## tab events {#group-tab}

### `tab.created` {#tab-created}

**Version:** 1

[`TabEvent`](/docs/reference/api/types#tabevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `name` | string \| `null` |  |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `tab.renamed` {#tab-renamed}

**Version:** 1

[`TabEvent`](/docs/reference/api/types#tabevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `name` | string \| `null` |  |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `tab.moved` {#tab-moved}

**Version:** 1

[`TabEvent`](/docs/reference/api/types#tabevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `name` | string \| `null` |  |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `tab.closed` {#tab-closed}

**Version:** 1

[`TabEvent`](/docs/reference/api/types#tabevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `name` | string \| `null` |  |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

### `tab.selected` {#tab-selected}

**Version:** 1

[`TabEvent`](/docs/reference/api/types#tabevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `handle` | string | yes |  |
| `name` | string \| `null` |  |  |
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |
| `workspace` | [`WorkspaceId`](/docs/reference/api/types#workspaceid) | yes |  |

## layout events {#group-layout}

### `layout.changed` {#layout-changed}

**Version:** 1

[`LayoutChanged`](/docs/reference/api/types#layoutchanged)

| Field | Type | Required | Description |
|---|---|---|---|
| `tab` | [`TabId`](/docs/reference/api/types#tabid) | yes |  |

## window events {#group-window}

### `window.opened` {#window-opened}

**Version:** 1

[`WindowEvent`](/docs/reference/api/types#windowevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

### `window.closed` {#window-closed}

**Version:** 1

[`WindowEvent`](/docs/reference/api/types#windowevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `window` | [`WindowId`](/docs/reference/api/types#windowid) | yes |  |

## phone events {#group-phone}

### `phone.paired` {#phone-paired}

**Version:** 1

[`PhoneEvent`](/docs/reference/api/types#phoneevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `name` | string | yes |  |

### `phone.renamed` {#phone-renamed}

**Version:** 1

[`PhoneEvent`](/docs/reference/api/types#phoneevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `name` | string | yes |  |

### `phone.removed` {#phone-removed}

**Version:** 1

[`PhoneEvent`](/docs/reference/api/types#phoneevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `name` | string | yes |  |

### `phone.pushed` {#phone-pushed}

**Version:** 1

[`PhonePushed`](/docs/reference/api/types#phonepushed)

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `reason` | [`AttentionReason`](/docs/reference/api/types#attentionreason) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |

### `phone.push_failed` {#phone-push-failed}

**Version:** 1

[`PhonePushFailed`](/docs/reference/api/types#phonepushfailed)

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `status` | integer (uint16) \| `null` |  | The last status of the push service. `None` when no answer came. |

### `phone.replied` {#phone-replied}

**Version:** 1

[`PhoneReplied`](/docs/reference/api/types#phonereplied)

| Field | Type | Required | Description |
|---|---|---|---|
| `device` | [`DeviceId`](/docs/reference/api/types#deviceid) | yes |  |
| `run` | [`RunId`](/docs/reference/api/types#runid) | yes |  |

## policy events {#group-policy}

### `policy.added` {#policy-added}

**Version:** 1

[`PolicyEvent`](/docs/reference/api/types#policyevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |
| `scope` | [`PolicyScope`](/docs/reference/api/types#policyscope) | yes |  |

### `policy.removed` {#policy-removed}

**Version:** 1

[`PolicyEvent`](/docs/reference/api/types#policyevent)

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |
| `scope` | [`PolicyScope`](/docs/reference/api/types#policyscope) | yes |  |

## Notification `events.event` {#notification-events-event}

[`EventNotification`](/docs/reference/api/types#eventnotification)

| Field | Type | Required | Description |
|---|---|---|---|
| `event` | [`Event`](/docs/reference/api/types#event) | yes |  |
| `subscription` | string | yes |  |

## Notification `events.truncated` {#notification-events-truncated}

[`EventsTruncated`](/docs/reference/api/types#eventstruncated)

| Field | Type | Required | Description |
|---|---|---|---|
| `cursor` | [`Cursor`](/docs/reference/api/types#cursor) | yes | The cursor to start from after a fresh snapshot. |
| `subscription` | string | yes |  |
