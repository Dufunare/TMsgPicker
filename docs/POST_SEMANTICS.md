# Post semantics and metric interpretation

TMsgPicker treats a Telegram media group as one logical **Post** for query, filtering,
sorting, display, and output limits. Telegram itself still stores each album member as
an independently addressable message.

## Interaction metrics

* One underlying message: use that message's metrics verbatim.
* Multiple members: **views** and **forwards** are the maximum of the available
  per-member values (not the sum). No observed value means `null`, never 0.
* **Comments** (`reply_info`) and **reactions** belong to one deterministic
  primary member: choose the earliest member with a non-empty caption/text,
  or the earliest member by date/message ID when none has text.
* A missing primary-member count stays unknown rather than silently borrowing
  from another member.
* Member metrics are retained unmodified and may be inspected via the
  hover-only badge on that media preview. The badge does not change the
  query-level Post aggregate.
* `provenance` records the member ID supplying each aggregate for internal
  debugging. These IDs are not presented in ordinary Post cards.

**Caveat:** This canonical member is an explicit *TMsgPicker* policy, not a
guarantee that all Telegram clients use the same member for album interaction
display. The maximum is a conservative aggregate, **not** the number of
distinct viewers or unique forwarding events.

## Content-type predicates

Content types are not numbers. Rather than comparing
`content_kind = "video"` on a potentially arbitrary first album member,
queries now support:

```json
{
  "kind": "and",
  "items": [
    { "kind": "contains_content", "content_kind": "video", "exists": true },
    { "kind": "contains_content", "content_kind": "photo", "exists": false }
  ]
}
```

`contains_content` examines **all members** of a Post, not just the
representative. `exists=false` is an explicit negation of presence, and AND,
OR and NOT remain composable. Unsupported content types are rejected when
validating the query. Retired PostKind, Album ID, ContentKind comparisons, and
Unix timestamp numeric comparisons do not appear in the editor. Date range is
expressed via scan scope; date remains an allowed sort key.

Older serialized filter trees referring to retired fields are not automatically
migrated. They should be regenerated using the current filter editor.

## Visual presentation

The Post card displays Post aggregates. Each individual media tile has a
small **view** and **forward** badge, revealed after approximately 420ms of
hover and hidden on pointer exit. Animation can be disabled in Advanced
Appearance; the interface also honors OS reduced-motion preferences.
