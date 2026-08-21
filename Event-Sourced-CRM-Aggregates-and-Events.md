**Event-Sourced Real Estate CRM**

*A Plain-Language Guide to Aggregates and Events*

Prepared for the engineering team

Contents

What We\'re Trying to Achieve

This document lays out the core data model for a real estate CRM built
using event sourcing --- a design where, instead of just storing the
current state of things (\"this listing costs \$450,000\"), the system
stores every individual fact that ever happened (\"the price was set to
\$420,000, then later changed to \$450,000\") as a permanent, ordered
log of events. The current state is just whatever you get when you
replay all those events in order.

We\'re using the term \"aggregate\" the way it\'s used in domain-driven
design: a cluster of related data that changes together, gets its own
permission rules, and is the unit the system keeps consistent. An
aggregate isn\'t the same thing as a database table --- it\'s a boundary
around a concept, like \"a single offer on a house\" or \"one person\'s
consent to be contacted.\" The events below are the specific, past-tense
facts that can happen to each aggregate --- written as things that
already occurred (OfferAccepted), never as commands to carry out
(AcceptOffer).

The reason this deserves a careful design pass, rather than just
building normal CRUD tables, is that real estate is one of the more
heavily regulated industries a CRM can sit inside. Nearly every
aggregate below exists because of a specific law: fair housing rules,
telemarketing consent rules, trust-accounting rules for client money,
anti-money-laundering rules, and state licensing rules, among others. A
design mistake here doesn\'t just mean a bug --- it can mean a fine, a
lost license, or a lawsuit years after the fact. Event sourcing is a
particularly good fit for that reality, because a complete,
tamper-evident history of exactly what happened and when is often the
actual legal deliverable, not just a debugging convenience.

The rest of this document walks through each aggregate one at a time:
what it is, in plain terms; why it\'s worth being its own aggregate
instead of just a field on something else; and what each of its events
means in practice. It closes with a few cross-cutting technical concerns
that come up specifically because we\'re combining event sourcing with a
heavily regulated domain, and a short list of open questions worth
digging into further before or during implementation.

Design Principles

A few ground rules shaped every aggregate in this document:

- **Aggregates are boundaries, not tables.** --- They exist to keep
  related data consistent and to define who\'s allowed to change what
  --- not to mirror a database schema. And every event is a fact about
  something that already happened, never an instruction to do something.

- **Personal information is separated from everything else.** --- A
  person\'s name, phone number, and email live only in the Party
  aggregate. Everything else refers to a person by ID. This is what
  makes it possible to honor a deletion request without destroying the
  historical record of a completed deal.

- **Compliance gets its own aggregates.** --- Licensing, consent,
  identity checks, fair-housing review, and record retention are each
  modeled as their own first-class aggregate --- not as extra fields
  bolted onto Listing or Contract. Each one has its own regulator, its
  own rules, and its own audit questions.

- **Every event carries a standard envelope of metadata.** --- Who
  actually did this, on whose behalf, when it really happened versus
  when we recorded it, where the request came from, and which version of
  the applicable rules was in effect. Without this, an event log looks
  complete but can\'t actually answer the questions a regulator or a
  court will ask.

Aggregates and Events

Party (Contact/Client)

*The only place a person\'s personal information is allowed to live*

A \"Party\" is any human being the system knows about: a buyer, a
seller, a tenant, an agent\'s contact, whoever. All of that person\'s
actual personal details --- name, phone, email --- live in exactly one
place. Every other part of the system just stores a reference (a party
ID) instead of copying that data around.

> ***Why this matters:** Privacy laws like GDPR and CCPA give people the
> right to have their personal data corrected or deleted. If a person\'s
> email address is copied into fifty different tables, honoring a
> deletion request means finding and fixing all fifty. If it lives in
> one place, deletion is a single, provable operation.*

Events

- **PartyRegistered** --- A new person was added to the system.

- **PartyRoleAssigned** --- We recorded what role this person plays in a
  deal --- buyer, seller, tenant, landlord, or lender.

- **ContactPointAdded / ContactPointVerified / ContactPointRemoved** ---
  A phone number, email address, or mailing address was added to the
  person\'s file, confirmed as genuine, or removed.

- **PartyDetailsCorrected** --- Someone fixed inaccurate information
  about this person. We never silently edit the old data --- we add a
  new event that supersedes it, so there\'s a permanent record of what
  changed, when, and by whom.

- **DuplicatePartiesMerged / PartyMergeReversed** --- We discovered the
  same real person had two separate records and combined them into one,
  or undid a merge that turned out to be wrong.

- **PartyPseudonymized** --- We honored a \"delete my data\" request by
  destroying the encryption key needed to read this person\'s personal
  details. The ID stays in place (so old records still make sense), but
  the private data behind it becomes permanently unreadable.

- **PartyProtectedStatusFlagged** --- We noted that this person is
  enrolled in a legal protection program --- for example, a state
  address-confidentiality program for domestic violence survivors ---
  which changes how we\'re allowed to handle their information.

ConsentRecord

*One record per person, per contact method, per purpose*

Every time someone agrees to be contacted a certain way, we write it
down --- one record for this specific person, using this specific
channel (call, text, or email), for this specific purpose. \"They filled
out a contact form once\" is not the same as \"they agreed to receive
marketing texts.\"

> ***Why this matters:** Telemarketing and spam laws (TCPA, CAN-SPAM,
> Do-Not-Call rules) make unwanted contact a legal liability, sometimes
> with per-message penalties. If we\'re ever challenged on whether
> someone consented, \"we have a checkbox in the database\" is a weak
> defense --- the actual wording they agreed to and proof they saw it is
> what holds up.*

Events

- **ConsentGranted** --- The person agreed to be contacted. We store the
  exact wording of what they agreed to, plus a fingerprint of the proof,
  not just a yes/no flag.

- **ConsentRevoked** --- They withdrew a permission they\'d previously
  given.

- **ConsentExpired** --- A permission timed out according to policy
  (consent isn\'t usually forever).

- **ConsentScopeNarrowed** --- They kept some permission but limited it
  --- for example, \"email is fine, but stop texting me.\"

- **DoNotCallScrubPerformed** --- We checked this phone number against
  the National Do-Not-Call list before calling it.

- **CallRecordingConsentCaptured** --- We obtained permission to record
  a phone call, required in states where both parties must agree to a
  recording.

- **MessageSuppressedByConsentPolicy** --- An outgoing message was
  automatically blocked because we didn\'t have valid consent to send
  it.

- **QuietHoursViolationPrevented** --- A message was blocked because it
  would have gone out outside the hours the law allows contacting
  someone.

Licensee (Agent) & Brokerage

*Tracks who is legally allowed to practice real estate, and under whose
supervision*

Real estate agents need a government-issued license to legally do their
job, and every licensed agent must operate under a supervising
brokerage. This aggregate is the system\'s source of truth for whether a
given agent is currently allowed to do the thing they\'re trying to do.

> ***Why this matters:** State licensing boards, not the agent, are the
> authority on whether a license is valid. An agent whose license lapsed
> or was suspended legally cannot create listings or draft contracts,
> and if the system lets them do it anyway, that exposes the brokerage
> to real liability.*

Events

- **AgentOnboarded** --- A new agent was added to the system.

- **LicenseVerifiedAgainstRegulator** --- We checked the agent\'s
  license directly against the state licensing board\'s own records ---
  not just trusting what the agent typed in.

- **LicenseRenewed / LicenseSuspended / LicenseRevoked /
  LicenseReinstated** --- The agent\'s license status changed.

- **BrokerageAffiliationStarted / Ended** --- The agent joined or left a
  brokerage.

- **SupervisingBrokerAssigned** --- A specific licensed broker was
  assigned to supervise this agent, as required by law.

- **ContinuingEducationCompleted / FairHousingTrainingCompleted** ---
  The agent completed a required training course.

- **ErrorsAndOmissionsCoverageRecorded / CoverageLapsed** --- The
  agent\'s professional liability insurance was recorded as active, or
  flagged as expired.

> ***Why this matters:** A subtle but important rule: when the system
> checks whether an agent was allowed to do something, it should check
> the license status as of the moment the action actually happened ---
> not the agent\'s current status. License status can change over time,
> and what matters in a dispute is what was true back then, not what\'s
> true today.*

AgencyRelationship (Representation Agreement)

*The formal, legal relationship between an agent and a client*

This tracks who is officially representing whom, and under what terms.
It\'s a distinct aggregate from Party because \"I know this person\" and
\"I am legally representing this person, with a signed agreement and
disclosed terms\" are very different things with very different legal
consequences.

> ***Why this matters:** Representing both the buyer and seller in the
> same deal (\"dual agency\") is restricted or flatly illegal in some
> states because of the conflict of interest. On top of that, a
> nationwide industry settlement now requires a signed written agreement
> with clear, specific compensation terms before an agent can even show
> a buyer a house --- skip that step and the agent risks being unpaid,
> or worse.*

Events

- **AgencyDisclosureDelivered** --- We told the client, in writing,
  exactly what kind of representation they\'re getting, and when we told
  them --- the timing itself is legally regulated.

- **BuyerRepresentationAgreementSigned / Amended / Terminated** --- The
  formal agreement between a buyer and their agent was signed, changed,
  or ended.

- **ListingAgreementSigned / Extended / Expired** --- The formal
  agreement between a seller and their agent was signed, extended, or
  ran out.

- **DualAgencyConsentObtained** --- Both sides knowingly agreed to be
  represented by the same agent, in a jurisdiction where that\'s
  allowed.

- **DualAgencyProhibitedInJurisdiction** --- The system blocked a
  dual-agency setup because it\'s illegal wherever the property is
  located.

- **CompensationTermsAgreed / CompensationTermsAmended** --- How much
  the agent gets paid was agreed to in writing, or later changed.

Property and Listing

*Two separate aggregates, on purpose*

A Property is the physical house or parcel of land --- it exists whether
or not it\'s currently for sale, and the same property can be listed for
sale multiple times over the years. A Listing is one specific \"for
sale\" advertisement of that property, active for a limited window of
time. Keeping these separate means a property\'s permanent facts (like
\"this house has lead paint\" or \"this parcel sits in a flood zone\")
persist across every listing it ever has, while price history and
marketing status stay scoped to just the current listing.

> ***Why this matters:** Facts about a physical property --- hazards,
> square footage, past issues --- matter for the life of the building,
> not just for one sale. Mixing that permanent information into a single
> time-boxed \"listing\" record would mean losing it every time the
> property changes hands.*

Events

- **PropertyRegistered** --- The property itself was added to the
  system.

- **ParcelIdentifierAssigned** --- The property\'s official government
  tax/map identifier was recorded.

- **PropertyCharacteristicRecorded** --- A fact about the property ---
  square footage, bedroom count, etc. --- was recorded, along with where
  that fact came from. If a fact is later disputed, knowing its source
  matters.

- **Pre1978ConstructionFlagged** --- The property was flagged as built
  before 1978, which automatically triggers federal lead-paint
  disclosure requirements.

- **HazardZoneDesignationRecorded** --- The property was flagged as
  sitting in a flood zone, wildfire zone, or similar hazard area.

- **HOAAssociationLinked** --- The property was connected to its
  homeowners\' association record.

- **--- Listing events below ---** --- The following events belong to
  individual listings of the property, not the property itself:

- **ListingCreated** --- A new \"for sale\" listing was created for the
  property.

- **ListPriceSet / ListPriceChanged** --- The asking price was set or
  changed. We keep the complete price history, not just the current
  number.

- **ListingPublishedToMLS** --- The listing was published to the shared
  regional database (the MLS) that other agents and brokerages see.

- **OfficeExclusiveElected** --- The seller specifically authorized
  keeping the listing private to one office instead of the shared
  database.

- **ListingCopyFairHousingReviewed /
  ListingCopyRejectedForProhibitedLanguage** --- The marketing
  description was checked for language that illegally discriminates (for
  example, \"perfect for a young family\"), and either passed or was
  rejected.

- **ListingStatusChanged** --- The listing moved between states --- for
  example, from \"active\" to \"pending.\"

- **ListingWithdrawn / Expired** --- The listing was pulled by the
  seller, or its term simply ran out.

Offer / PurchaseContract

*One of the most legally sensitive parts of the whole system*

This tracks a specific buyer\'s offer on a property, all the way through
negotiation to a signed contract and every condition attached to it.

> ***Why this matters:** An agent has a legal duty to present every
> offer to the seller --- skipping or downplaying one is a serious
> violation. Just as important, this is where discrimination claims tend
> to get proven or disproven: regulators compare how different offers,
> from different buyers, were actually handled.*

Events

- **OfferDrafted** --- A buyer\'s offer was written up.

- **OfferPresentedToSeller** --- We recorded proof that the seller was
  actually shown the offer. Presenting every offer is a legal duty, not
  optional bookkeeping --- this event is the evidence that it happened.

- **CounterOfferIssued** --- The seller countered with different terms.

- **OfferAccepted** --- The offer was accepted, referencing the exact
  signed document and its version.

- **OfferRejected** --- The offer was turned down. Rejected offers are
  never deleted --- comparing how different offers were treated is
  exactly how discrimination claims are proved or disproved.

- **ContractExecuted** --- The purchase contract was fully signed by
  everyone. Its \"effective date\" becomes the anchor point that every
  other deadline in the deal counts from.

- **ContingencyDeadlineSet** --- A condition of the deal --- for
  example, \"the inspection must pass by this date\" --- got its real
  deadline calculated, including which jurisdiction\'s counting rules
  (business days vs. calendar days) were used.

- **ContingencySatisfied / Waived / DeadlineMissed** --- That condition
  was met, voluntarily given up, or missed.

- **InspectionCompleted / AppraisalReceived / FinancingApprovalReceived
  / TitleCleared** --- Standard milestones in the deal were completed.

- **RightOfRescissionExercised** --- The buyer used a legal right to
  cancel the deal within a required cooling-off period.

- **ContractAmended / Terminated / Closed** --- The contract was
  changed, called off, or the sale finished successfully.

DisclosurePackage

*Separate from Document --- this is about the obligation, not the file*

A \"disclosure\" is a legal promise the seller made to tell the buyer
certain facts about the property. This aggregate is deliberately kept
separate from the Document aggregate, because \"do we have the PDF\" and
\"did we actually fulfill our legal duty to inform the buyer\" are two
different questions. You can have the file sitting in storage and still
be unable to prove anyone ever delivered it or that the buyer
acknowledged reading it --- and that second part is what actually
matters legally.

> ***Why this matters:** Disclosure timing and delivery are directly
> regulated --- federal lead-paint rules, state seller-disclosure laws,
> and \"stigmatized property\" rules (for example, a death on the
> property) all hinge on proving something specific was delivered, when,
> and that it was acknowledged.*

Events

- **DisclosureObligationDetermined** --- The system figured out which
  disclosures are legally required for this property and jurisdiction,
  and recorded exactly which version of the rules it used to decide.

- **LeadBasedPaintDisclosureDelivered** --- The federally required
  lead-paint disclosure was delivered (this only applies to homes built
  before 1978).

- **SellerPropertyDisclosureCompleted / Amended** --- The seller\'s
  disclosure form about the property\'s condition was filled out, or
  later updated.

- **DisclosureDelivered / Viewed / Acknowledged /
  AcknowledgmentRefused** --- The disclosure was sent to the buyer,
  opened by them, formally confirmed as received, or the buyer refused
  to sign off on it.

- **MaterialChangeDetected → RedisclosureRequired** --- Something
  important about the property changed, which legally requires informing
  the buyer all over again.

- **DisclosureDeadlineMissed** --- A required disclosure was not
  delivered in time.

EscrowAccount / TrustLedger

*A strict, append-only ledger for other people\'s money*

This tracks real client money --- earnest money deposits, closing funds
--- held by the brokerage on someone else\'s behalf. It\'s built like an
accounting book you\'re never allowed to erase from, only add correcting
entries to.

> ***Why this matters:** Mixing client money with the brokerage\'s own
> operating money (\"commingling\") is one of the fastest ways an agent
> or broker loses their license. Wire fraud targeting real estate
> closings --- where scammers send fake \"updated wire instructions\"
> --- is also extremely common and can cost a buyer their entire down
> payment in minutes.*

Events

- **EarnestMoneyReceived** --- The buyer\'s good-faith deposit came in.

- **FundsDepositedToTrustAccount** --- Money was placed into the trust
  account.

- **DisbursementAuthorized / FundsDisbursed** --- Paying money out was
  approved, and then actually sent.

- **LedgerEntryPosted / LedgerEntryReversed** --- A ledger line was
  added. If it turns out to be wrong, we add a reversing entry rather
  than editing the original --- exactly like real bookkeeping.

- **TrustAccountReconciled** --- We checked the ledger against the
  actual bank balance and confirmed they match.

- **ShortfallDetected** --- The ledger and the actual bank balance
  didn\'t match --- money appears to be missing.

- **CommingledFundsDetected** --- Client money was found mixed in with
  the brokerage\'s own operating funds, which isn\'t allowed.

- **WireInstructionsVerifiedByCallback** --- Before wiring money, we
  called the recipient at a phone number we already knew to be genuine,
  to confirm the wire instructions weren\'t sent by a scammer.

- **WireFraudWarningDelivered** --- We warned a client about wire fraud
  risk before they sent money.

- **ClosingDisclosureDelivered** --- The final financial breakdown of
  the deal was delivered to the buyer. Federal rules require this at
  least three business days before closing --- the timestamp on this
  event is itself the proof of compliance.

- **EscrowDisputeOpened / InterpleaderFiled / EscrowClosed** --- A
  disagreement over the money arose, was handed to a court to decide
  who\'s owed what, or the account was closed out normally.

Document / DocumentEnvelope

*Proving a file hasn\'t been tampered with*

This handles the actual files --- contracts, disclosures, signed PDFs
--- and the proof that they\'re authentic. The real file bytes live in
secure, write-once storage; the events themselves only hold pointers to
those files and cryptographic fingerprints of them.

> ***Why this matters:** E-signature law (ESIGN/UETA) and
> chain-of-custody requirements mean it\'s not enough to have a file ---
> you need to be able to prove which exact version was signed, by whom,
> when, and that it hasn\'t changed since.*

Events

- **DocumentCreated** --- A new document was added to the system.

- **DocumentHashRecorded** --- We calculated a cryptographic fingerprint
  of the file, so we can later prove whether it was altered.

- **DocumentVersionAdded / DocumentSuperseded** --- A new version of the
  document was added, replacing an older one.

- **SignatureRequested** --- We asked someone to sign the document.

- **SignerAuthenticated** --- We verified the signer was actually who
  they claimed to be.

- **ElectronicConsentRecorded** --- The signer agreed to sign
  electronically instead of on paper, which the law requires us to
  obtain separately.

- **DocumentSigned** --- The signature was completed.

- **SignatureCertificateReceived** --- We received the technical
  proof-of-signature certificate from the e-signature provider.

- **DocumentVoided / Archived / Disposed** --- The document was
  cancelled, moved into long-term storage, or deleted according to the
  retention schedule.

ComplianceCase (KYC / AML / Sanctions)

*Background checks with no \"we didn\'t know\" excuse*

This handles legally required background checks on the people involved
in a deal: confirming we\'re not doing business with anyone on a
government sanctions list, and identifying who\'s really behind a
purchase when it\'s not financed with a loan (an easy way to hide
illicit money).

> ***Why this matters:** Sanctions violations are strict liability ---
> there\'s no legal defense of \"we didn\'t realize.\" Federal rules
> also specifically target all-cash purchases by companies or trusts,
> because that\'s a well-known method for laundering money through real
> estate.*

Events

- **ScreeningInitiated** --- We started checking a person against
  government sanctions and watch lists.

- **SanctionsListMatchFound / MatchClearedAsFalsePositive** --- A
  possible match turned up, and it was either confirmed as real or
  cleared as a false alarm (a common name collision, for example).

- **TransactionBlockedForSanctions** --- A deal was stopped because of a
  genuine sanctions match.

- **IdentityVerified / IdentityVerificationFailed** --- We confirmed, or
  failed to confirm, that someone is who they say they are.

- **BeneficialOwnershipCollected** --- For a company or trust buying
  property, we identified the actual human beings who ultimately own or
  control it.

- **SourceOfFundsDocumented** --- We recorded proof of where the
  buyer\'s money actually came from.

- **AllCashTransferFlagged** --- A non-financed property transfer to a
  company or trust was flagged, since federal rules require extra
  reporting on these specifically.

- **PEPIdentified** --- We identified that someone involved is a
  \"politically exposed person\" --- a government official or their
  close associate --- which requires extra scrutiny.

- **RiskRatingAssigned** --- A risk level was assigned to this case.

- **RegulatoryReportFiled** --- A required report was filed with
  regulators. The event only stores a pointer to the filing, never its
  sensitive contents.

- **ComplianceClearanceGranted** --- The case passed all checks and was
  cleared to proceed.

> ***Why this matters:** Events tied to Suspicious Activity Reports need
> extra security and must be hidden from ordinary CRM users. Letting a
> suspect find out they\'re under investigation (\"tipping off\") is
> itself a separate crime --- so these events can\'t just live in the
> normal activity feed.*

Lead / Opportunity

*A potential client before they become a formal one*

This tracks someone who filled out a form or called in, before there\'s
any formal representation agreement in place.

> ***Why this matters:** How a lead is handled --- assigned, qualified,
> or turned away --- can itself become evidence in a discrimination
> claim, so the reasoning behind those decisions has to be recorded
> carefully.*

Events

- **LeadCaptured** --- A new lead came in, along with where it came from
  and any consent that was captured alongside it.

- **LeadAssignedToAgent** --- The lead was routed to a specific agent.

- **LeadQualified** --- We determined the lead is a real, serious
  prospect.

- **LeadDisqualified** --- We decided not to pursue the lead. The reason
  must come from a pre-approved list, never free text --- a note like
  \"seemed like they couldn\'t afford it\" typed into a free-text field
  can itself become evidence of illegal discrimination.

- **LeadConverted** --- The lead became an actual client or deal.

- **LeadArchived** --- The lead was closed out. This also starts the
  clock on how long we\'re allowed to keep their data.

Showing

*Key evidence in fair-housing investigations*

This tracks when a buyer is shown a property in person.

> ***Why this matters:** Regulators investigating illegal \"steering\"
> --- guiding buyers toward or away from certain neighborhoods based on
> race or other protected characteristics --- look directly at patterns
> of which buyers were shown which properties. This is often the single
> most important piece of evidence in that kind of case.*

Events

- **ShowingRequested / Approved / Declined / Completed** --- The
  lifecycle of scheduling and completing a home showing. Declines are
  recorded with a reason code, not free text, for the same
  discrimination-evidence reason as lead disqualification.

- **PropertiesShownRecorded** --- The actual list of properties shown to
  a given buyer --- the core evidence used to prove or disprove a
  steering claim.

FairHousingReview

*A dedicated workflow, not just a note on a listing*

This gives potential discrimination issues their own tracked process,
separate from wherever the underlying activity happened.

> ***Why this matters:** Handling fair-housing concerns consistently,
> with a documented review process, is itself part of demonstrating
> good-faith compliance if a regulator ever asks how the company handles
> these situations.*

Events

- **PotentialSteeringPatternDetected** --- An automated or manual review
  flagged a pattern that looks like discrimination.

- **ReviewOpened / FindingRecorded / Closed** --- A formal review of the
  flagged pattern was started, its conclusion recorded, and it was
  closed out.

- **AccommodationRequestReceived / Granted / Denied** --- A tenant or
  buyer asked for a reasonable accommodation --- for example, related to
  a disability --- and that request was granted or denied, with a
  reason.

- **AssistanceAnimalRequestHandled** --- A request to have a service or
  support animal was processed.

- **SourceOfIncomeInquiryFlagged** --- A question about how someone pays
  --- for example, a housing voucher --- was flagged, since roughly
  twenty states ban discriminating on that basis.

- **SegmentRejectedForProtectedProxy** --- An ad-targeting audience was
  blocked because it was effectively targeting or excluding people by a
  protected characteristic, even without saying so directly. Keeping a
  record that we caught and blocked this is itself valuable evidence of
  good-faith compliance.

CommissionOrReferral

*How agents and brokerages get paid, and who\'s allowed to refer
business*

This tracks compensation arrangements and referral relationships between
agents and brokerages.

> ***Why this matters:** Paying kickbacks for real estate referrals is a
> federal crime (RESPA), and paying commission to someone without a
> valid license is also illegal --- so this isn\'t just accounting,
> it\'s a set of hard legal boundaries the system needs to enforce.*

Events

- **CompensationAgreementCreated** --- The terms of how someone gets
  paid were set up.

- **CommissionCalculated** --- The commission amount was calculated,
  referencing which formula and version was used.

- **ReferralFeeApproved / Rejected** --- A referral fee was checked
  against anti-kickback rules and either approved or blocked.

- **PaymentToUnlicensedPartyBlocked** --- The system stopped a payment
  because the recipient doesn\'t have a valid real estate license.

- **CommissionPaid** --- The commission was actually paid out.

- **1099Issued** --- The required tax form was issued for the payment.

PrivacyRequest (DSAR)

*A person\'s formal request under privacy law*

\"DSAR\" stands for Data Subject Access Request --- the formal legal
term for someone asking to see, correct, or delete the data a company
holds on them. This aggregate tracks that request from start to finish.

> ***Why this matters:** Privacy laws give these requests hard legal
> deadlines and require proof that the requester\'s identity was
> verified before handing over or deleting anything --- get either step
> wrong and it becomes its own compliance failure.*

Events

- **PrivacyRequestReceived** --- Someone asked to exercise a privacy
  right.

- **RequesterIdentityVerified** --- We confirmed the request genuinely
  came from that person, so we don\'t leak someone\'s data to an
  impersonator.

- **RequestDeadlineCalculated** --- We calculated the legal deadline for
  responding to this request.

- **ErasureScopeDetermined** --- We figured out exactly what can, and
  can\'t, legally be deleted.

- **ErasurePartiallyDenied** --- We denied part of the deletion request,
  with the specific legal reason recorded --- this event is essentially
  our defense if the person disputes the decision.

- **ErasureExecuted** --- The deletion was carried out, and we recorded
  exactly which systems and encryption keys were involved.

- **AccessPackageGenerated** --- We compiled a copy of everything we
  have on this person, to hand over to them.

- **SaleOrSharingOptOutApplied** --- We honored a request to stop
  selling or sharing this person\'s data.

- **PrivacyRequestCompleted** --- The request was fully resolved.

RetentionPolicy & LegalHold

*How long we\'re required to keep things, and when that gets overridden*

This tracks how long different kinds of records legally must (or may) be
kept, and can pause deletion entirely when there\'s an active lawsuit or
investigation.

> ***Why this matters:** Different rules demand different retention
> periods for the same record --- state brokerage rules might require
> three to five years, federal anti-money-laundering rules require five,
> lending-related records require just over two. The system has to
> satisfy all of them at once, and never delete something a court has
> ordered preserved.*

Events

- **RetentionClassAssigned** --- A record was tagged with which
  retention rule applies to it.

- **RetentionClockStarted** --- The countdown to \"this can now be
  deleted\" began --- importantly, started by a real business event,
  like a contract closing, not just by when the database row was
  created.

- **DispositionDateCalculated** --- The exact date this record becomes
  eligible for deletion was calculated.

- **LegalHoldIssued / Released** --- A hold was placed on deleting
  certain records because of a lawsuit or investigation, and later
  lifted.

- **DeletionJobBlocked** --- A scheduled deletion was automatically
  stopped because of an active legal hold --- this is exactly the kind
  of record that protects the company if it\'s ever accused of
  destroying evidence.

- **RecordsDeleted / CryptographicErasureCompleted** --- Records were
  deleted normally, or erased by destroying the encryption key that made
  them readable.

- **DispositionCertificateIssued** --- A formal certificate was
  generated proving the deletion happened as required.

> ***Why this matters:** The simple version of the retention rule: for
> any given record, keep it as long as the longest of all the individual
> legal requirements that apply to it --- and a legal hold always
> overrides that calculation, no matter what the normal schedule says.*

Why Event Sourcing Makes Compliance Both Easier and Trickier

Storing a complete history of everything that happened is a natural fit
for an industry built on proving what occurred and when. But it also
creates a few specific technical puzzles worth calling out on their own,
since they cut across every aggregate above rather than belonging to
just one.

The \"right to be forgotten\" versus a log you\'re never supposed to
alter

An event log is meant to be append-only --- you add to it, you don\'t
edit or delete from it. Privacy law sometimes requires the opposite:
permanently erasing someone\'s personal data on request. The way to
satisfy both at once is called crypto-shredding: encrypt each person\'s
private data with its own individual encryption key, and store that key
somewhere separate from the event itself. Deleting that one key makes
the person\'s data permanently unreadable everywhere it appears, without
touching anyone else\'s history and without breaking the tamper-evidence
checks that protect the log\'s integrity. It\'s also worth knowing that
privacy law itself recognizes limits here --- records tied to a closed,
legally-required transaction are often exempt from erasure entirely,
while a stale, unconverted lead usually isn\'t. And simply adding a
\"this person asked to be forgotten\" event that the reporting layer
quietly filters out doesn\'t satisfy anyone --- the underlying data has
to actually become unreadable.

Two different clocks

Every event should record both when something actually happened in the
real world (occurredAt) and when the system found out about it and
recorded it (recordedAt). These are often different --- a signed
document might get uploaded a day after it was actually signed. Legal
deadlines, cancellation windows, and license validity all depend on the
real-world timing, so conflating the two clocks can quietly produce
wrong answers to legally important questions.

Fix forward, never rewrite history

When a mistake is discovered, the fix is a new event that says \"this
corrects that earlier event,\" never a silent edit of the original.
Quietly changing a timestamp or a recorded fact in a system meant to be
a regulated historical record isn\'t just bad practice --- it can amount
to falsifying evidence.

Version the rules, not just the software

Real estate law varies by state and changes over time. Every event that
depended on some rule (a disclosure deadline, a contingency window)
should record exactly which version of that rule was applied. That way,
if the system\'s state is ever rebuilt from scratch by replaying the
whole event history, it reproduces the same decisions it made the first
time --- instead of accidentally re-evaluating old events against
today\'s rules.

Replaying history must never repeat real-world actions

If the system ever needs to rebuild its current state by replaying every
event from the beginning, that replay must never accidentally re-trigger
a real side effect --- resending a text message that\'s regulated by
consent law, or re-filing a report with a government agency. Anything
that reaches outside the system has to be guarded so it only fires once,
no matter how many times the history is replayed.

Where the Three AI Models Disagreed

This design was synthesized from three different frontier models, and
while they largely agreed on the aggregates above, they disagreed on a
few structural questions. Here\'s what they disagreed about and how it
was resolved.

How big should the \"Transaction\" aggregate be?

One model proposed a single large Transaction aggregate covering an
entire deal end to end. The other two warned this becomes an
unmanageable \"god object\" --- one aggregate trying to own too much,
with too many different concurrency and permission needs. Resolution:
split it up. Transaction becomes a thin coordinator that just tracks the
overall status of a deal, while Offer, Contract, Disclosures, Escrow,
and Documents each remain independent aggregates with their own owners
and rules.

Is the event log itself the audit trail?

One model argued the event log is sufficient on its own as the audit
record. The other two disagreed. Resolution: the event log is necessary
but not sufficient. It should be supplemented with cryptographic
checksums that make tampering detectable, the actual signed document
files stored in unchangeable storage, and logging of who viewed
sensitive data --- not just who changed it.

Should the log be absolutely, permanently unchangeable?

One model insisted nothing should ever be deleted, under any
circumstances. The other two argued for a controlled, tightly audited
exception process. Resolution: promise that nothing is altered through
normal, everyday operation, but allow a rare, fully-logged exception
process for genuine emergencies --- for example, someone accidentally
pasting their Social Security number into a free-text notes field.

Open Questions Worth Researching Separately

A few topics came up as worth digging into on their own, outside the
scope of this document:

- **Data ownership when an agent switches brokerages.** --- Who owns the
  historical client and deal data an agent generated while affiliated
  with a brokerage they\'ve since left?

- **Provenance for migrated legacy data.** --- When old data is imported
  from a previous system, how do we preserve a trustworthy record of
  where it originally came from?

- **Global Privacy Control signals.** --- How should the system handle
  browser-level \"do not sell my data\" signals automatically, rather
  than relying only on explicit requests?

- **Disaster recovery requirements.** --- Given that this event store is
  effectively the company\'s official legal record, what recovery point
  and recovery time objectives are appropriate for it?

Closing Note

The aggregates and events above are a starting point for implementation,
not a finished specification --- schemas, exact field lists, and the
sequencing of a phased build (what\'s legally required at launch versus
what can come later) are the natural next steps. But the boundaries
themselves --- especially keeping personal data isolated in Party,
giving compliance concerns their own aggregates, and treating the event
envelope (who, when, on whose behalf, under which rules) as
non-negotiable --- are the decisions worth getting right early, since
they\'re the hardest to change later without rewriting history that\'s
meant to be permanent.
