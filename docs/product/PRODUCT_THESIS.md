# Product Thesis

## Document purpose

This document describes the current product direction for the AI-first real-estate relationship platform.

It is intentionally shorter than a full product requirements document. It answers:

- who the initial customer is;
- which problem we are solving;
- why a customer would leave Follow Up Boss;
- what the product must do exceptionally well;
- what the initial product includes;
- what is deliberately deferred;
- how we will measure whether the thesis is working.

Accepted decisions in `docs/decisions/DECISION_LOG.md` take precedence over this document.

---

## 1. Product statement

We are building an AI-first real-estate relationship platform for residential real-estate teams.

The platform helps agents and team leaders manage the complete working relationship from new lead through active client communication.

It uses Follow Up Boss as the primary reference product because Follow Up Boss has a strong real-estate-specific operating loop:

```text
Lead arrives
  → identify and deduplicate
  → route to the right agent
  → tell the agent who needs attention
  → call, text, or email
  → capture the interaction
  → determine the next action
  → repeat
```

We will preserve that clarity while improving the migration experience, daily workflow, mobile experience, realtime communication, and AI interaction model.

The product is not “Follow Up Boss rewritten in Rust.”

The product is:

> A real-estate relationship operating system that tells agents who needs attention, lets them act through natural language or voice, captures communication automatically, and gives the brokerage a reliable system of record.

---

## 2. Initial customer

The initial customer is a residential real-estate team with approximately:

- 10–50 agents;
- one or a small number of offices;
- a broker, team leader, or operations manager who owns the CRM decision;
- Zillow, Realtor.com, website, referral, and manually entered leads;
- current Follow Up Boss usage;
- calling, SMS, Gmail or Microsoft 365, and calendar requirements;
- a need for better mobile and daily follow-up behavior.

The first product is not designed for a very large national brokerage with hundreds of offices and deeply customized back-office systems.

The product model must not prevent later expansion, but early implementation should optimize for the 10–50-agent team.

---

## 3. Economic buyer and daily user

### Economic buyer

The economic buyer is usually:

- a broker;
- a team leader;
- an owner;
- an operations leader.

They care about:

- whether paid leads are worked;
- response time;
- assignment and reassignment;
- retention of customer history;
- agent departure;
- communication continuity;
- reporting;
- data ownership;
- migration risk;
- accountability;
- security and compliance.

### Daily user

The daily user is usually:

- an agent;
- an ISA;
- a team lead who also works leads.

They care about:

- who to contact next;
- how quickly they can call or message;
- whether the CRM updates itself;
- how much data entry is required;
- whether the mobile application is reliable;
- whether the system interrupts them too often;
- whether the product helps them close business.

The product must satisfy both groups.

A system that brokers love but agents avoid will fail.

A system agents like but brokers cannot control or trust will also fail.

---

## 4. Primary switching thesis

The first two switching advantages are:

### 4.1 Safe migration from Follow Up Boss

Migration is not a setup utility. It is a product capability.

A customer should be able to understand:

- what was discovered in Follow Up Boss;
- what was imported;
- what was preserved;
- what was reconstructed from Gmail or Microsoft 365;
- what Follow Up Boss did not expose;
- what requires human review;
- whether assignments, source history, custom fields, notes, tasks, and communication history reconcile.

The system must never silently discard unsupported data.

A migration report should make the move feel controlled rather than risky.

### 4.2 An AI Operator that materially reduces navigation

The AI Operator should let a user describe the outcome they want instead of navigating through many screens.

Examples:

- “Who should I call next?”
- “Why is Sarah first?”
- “Call her.”
- “Text her that I am five minutes late.”
- “Move her to Active Buyer.”
- “Remind me Friday.”
- “Show uncontacted Zillow leads.”
- “Prepare a reassignment plan for Alex.”

The Operator is not a chatbot with direct database access.

It uses the same typed application commands and queries as the web and mobile applications.

The application remains responsible for authorization, validation, risk classification, confirmation, execution, and audit.

---

## 5. Supporting differentiation

The supporting advantages are:

### 5.1 First-class native mobile applications

The iOS and Android applications are primary product surfaces.

The initial native experience focuses on:

- Today;
- AI Operator;
- People;
- Person detail;
- inbound and outbound calls;
- SMS;
- tasks;
- appointments;
- high-priority notifications.

Administration and migration can remain web-only initially.

### 5.2 Integrated communication

The platform should make communication part of the relationship record.

Initial communication capabilities include:

- inbound and outbound PSTN calling;
- browser calling;
- native mobile calling;
- SMS and MMS;
- Gmail;
- Microsoft 365;
- Google Calendar;
- Microsoft Calendar;
- future in-app chat.

Calls, messages, tasks, notes, and appointments appear in a unified Person timeline.

### 5.3 Broker control and continuity

The brokerage needs understandable control over:

- lead assignment;
- simple routing;
- business-hour policies;
- team and company numbers;
- communication history;
- reassignment;
- agent departure;
- auditability;
- customer data continuity.

Historical activity remains attributed to the person who performed it, even when responsibility changes.

### 5.4 Realtime behavior without notification overload

Connected clients receive realtime application updates.

Backgrounded applications receive high-importance push notifications.

The notification system should prioritize interruption carefully.

The Today engine should reduce noise by consolidating ordinary work into a prioritized queue.

---

## 6. The core product loop

The core loop is:

```text
Lead enters
  → Organization is resolved
  → Person is resolved or created
  → Inquiry is created
  → source attribution is preserved
  → responsibility is assigned
  → Today work is created or updated
  → agent is notified appropriately
  → agent calls, texts, or emails
  → interaction is recorded
  → relationship state is updated
  → next work is determined
```

The first implementation work should prove this loop end to end.

The product should not begin by building isolated screens or broad feature catalogs.

---

## 7. Person as the relationship center

The Person is the center of the CRM.

A Person can have:

- names;
- contact methods;
- relationships;
- custom fields;
- tags;
- multiple Inquiries;
- source history;
- assignment history;
- stage history;
- calls;
- SMS;
- email;
- notes;
- tasks;
- appointments;
- website and property activity;
- future Deals;
- a future authenticated client identity.

Important separations:

```text
Person != Inquiry
Person != Deal
Person != authenticated Identity
```

A later Inquiry must not overwrite original source attribution.

A CRM Person does not automatically receive login access.

---

## 8. Today

> Amended by D-043 (2026-08-28): Today's inputs become visible and
> configurable — smart lists can feed Today as explainable work
> sources, and the built-in reasons below become org-tweakable system
> feeds expressed in the same filter vocabulary. Determinism and
> explainability are retained; "no secret priority" holds.

Today is the agent’s daily work queue.

It should answer:

> What should I do next, and why?

Initial ranking is deterministic and explainable.

Examples of reasons include:

- new Inquiry;
- no contact attempt;
- missed call;
- overdue task;
- stale active relationship;
- recent website activity;
- recent property activity.

The model may explain a result, but it does not secretly decide priority.

A work item should display:

- Person;
- recommended action;
- priority;
- business reasons;
- age or due time;
- available actions.

Merely opening a Person should not automatically count as meaningful work.

Detailed completion, snooze, and dismissal semantics will be specified separately.

---

## 9. AI Operator interaction model

The product is conversation-first, not chat-only.

The Operator can receive:

- typed requests;
- push-to-talk voice requests;
- current-screen context;
- trusted user and Organization context;
- structured CRM query results.

The Operator can return:

- concise prose;
- Person cards;
- Today lists;
- call actions;
- confirmation sheets;
- schedule choices;
- reassignment plans;
- migration issue lists.

Use conversation when describing intent is easier.

Use structured screens when inspecting dense information is easier.

### Action levels

#### Read-only

Execute immediately.

#### Low-risk and reversible

Execute and provide an action receipt or Undo where practical.

#### Consequential or bulk

Preview the impact and require confirmation.

The Operator must never directly modify tables or bypass normal authorization.

---

## 10. Initial product scope

The initial product includes:

### CRM core

- Organization, optional Office, Team, User, and membership;
- Person;
- contact methods;
- relationships;
- typed custom fields;
- tags;
- stages;
- Inquiry and source attribution;
- assignment and reassignment;
- notes;
- tasks;
- timeline;
- deterministic Today.

### Communication

- inbound and outbound calling;
- browser calling;
- native mobile calling;
- simple call routing;
- SMS and MMS;
- Gmail;
- Microsoft 365;
- Google Calendar;
- Microsoft Calendar;
- recordings, consent policy, transcription, summaries, and timeline entries as those capabilities mature.

### Realtime and notifications

- Centrifugo for connected application realtime;
- APNs and FCM for important background notifications;
- browser Web Push where appropriate;
- native incoming-call handling.

### AI Operator

Initial useful actions should include:

- search People;
- retrieve a Person;
- retrieve Today;
- retrieve the next work item;
- explain priority;
- change stage;
- create or complete a task;
- assign or reassign a Person;
- start a call;
- send a text;
- draft or send email under the applicable confirmation policy;
- schedule an appointment.

### Migration

- raw Follow Up Boss source preservation;
- mapping;
- validation;
- reconciliation;
- delta import;
- cutover support;
- explicit unsupported-data reporting.

---

## 11. Deliberately deferred capabilities

The initial product does not require:

- IDX website platform;
- advertising platform;
- full transaction-management system;
- sophisticated commission accounting;
- property recommendation engine;
- general-purpose workflow builder;
- autonomous AI nurture;
- full client mobile application;
- video calling;
- every Follow Up Boss report;
- every legacy Follow Up Boss edge case;
- advanced inferred presence;
- active-active multi-region infrastructure.

A deferred capability may be researched or represented in the model, but it must not distract from the core loop.

---

## 12. Product principles

### 12.1 Intent before navigation

When describing intent is easier than navigating, the user should be able to describe the intent.

### 12.2 Visual interfaces remain important

The Operator does not replace every screen.

Lists, pipelines, calendars, migration review, routing configuration, and bulk administration often require structured visual interfaces.

### 12.3 Explain business reasons

Show why the system recommends an action.

Do not expose hidden model reasoning.

### 12.4 No silent data loss

Migration, synchronization, and integration failures must be visible and reconcilable.

### 12.5 One business capability, multiple clients

Web, iOS, Android, Operator, integrations, and future automation use the same application command/query layer.

### 12.6 Realtime is delivery, not truth

Clients recover authoritative state from the application.

### 12.7 Mobile interruption is expensive

Use high-priority push only when interruption is justified.

### 12.8 Build for auditability

Important assignments, routing decisions, consent, calls, administrative actions, and Operator actions must be explainable later.

### 12.9 Prefer a narrow excellent workflow

A small complete workflow is more valuable than many disconnected screens.

### 12.10 Avoid infrastructure theater

Existing infrastructure choices are accepted, but product development must not add services or abstractions without a concrete requirement.

---

## 13. Initial proof of value

The first convincing product demonstration is:

```text
A new lead enters
  → Person and Inquiry are created correctly
  → source is preserved
  → agent is assigned
  → Today updates
  → connected clients update in realtime
  → background mobile receives appropriate push
  → agent asks “Who should I call next?”
  → Operator explains the next Person
  → agent says “Call her”
  → LiveKit and Telnyx place the PSTN call
  → the call is recorded in the timeline
  → Today advances
```

This flow tests the product thesis and the most important architecture boundaries at the same time.

---

## 14. Success criteria

The initial product is succeeding when:

### Migration

- a real Follow Up Boss account can be imported without silent loss;
- the customer receives a useful reconciliation report;
- the import can be repeated safely;
- cutover supports a final delta import.

### Agent adoption

- agents open Today regularly;
- agents can identify their next action quickly;
- agents use the Operator for real work, not only novelty questions;
- mobile calling and messaging are reliable;
- manual CRM data entry decreases.

### Broker value

- response time is visible;
- assignments and reassignment are understandable;
- agent departure does not destroy continuity;
- communication history stays with the organization according to accepted policy;
- customer data and administrative actions are auditable.

### Operator quality

- correct tool selection;
- correct Person resolution;
- correct arguments;
- no unauthorized actions;
- low unnecessary-clarification rate;
- useful response latency;
- clear receipts and confirmations.

### Product quality

Measure end-to-end interaction latency, not only API server time.

Important experiences include:

- opening Today;
- opening a Person;
- finding a Person;
- changing stage;
- starting a call;
- receiving an incoming call;
- sending a text;
- receiving a realtime update;
- launching the native applications.

---

## 15. Current validation risks

The most important risks are:

1. We may misunderstand which Follow Up Boss capabilities prevent customers from switching.
2. Migration fidelity may be limited by data Follow Up Boss does not expose.
3. Calling, SMS, email, and calendar integrations contain provider and compliance edge cases.
4. The AI Operator may feel slower or less predictable than direct UI for common tasks.
5. Agents may resist another system even when the architecture is better.
6. The client-facing application hypothesis is not yet validated.
7. Solo development with parallel coding agents may produce integration debt without strict specifications and review.
8. SOC 2 readiness requires operating evidence, not only secure code.

These risks should be tested through working vertical slices and real design partners rather than answered through more architecture alone.

---

## 16. Initial product objective (achieved)

> Status 2026-08-29: this objective was met — the proof chain below
> shipped across Slices 002–005, and calling shipped as Slice 006.
> Current status lives in `docs/plans/PROJECT_STATE.md`.

The immediate objective is not to build the entire CRM.

It is to establish the repository, development environment, shared contracts, and first vertical slice needed to prove:

```text
New lead
  → correct CRM state
  → Today
  → realtime delivery
  → Operator retrieval and explanation
```

Calling becomes the next vertical slice after that foundation works.
