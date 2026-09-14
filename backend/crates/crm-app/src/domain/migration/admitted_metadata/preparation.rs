//! Durable retained-only preparation. Every transaction has a persisted keyset
//! checkpoint and settles its exact logical byte delta with that checkpoint.
use super::super::admitted_metadata_worker as w;
use super::*;
use crate::{
    domain::envelope::Origin,
    ids::{CorrelationId, UserId},
};
use sqlx::{postgres::PgRow, PgConnection};

#[derive(Clone, Copy)]
struct Prep {
    org: OrganizationId,
    import: Uuid,
    plan: Uuid,
    report: Uuid,
    snapshot: Uuid,
    account: i64,
    admission: Uuid,
    boundary: i64,
}
impl Prep {
    async fn issues(&self, c: &mut PgConnection, reasons: &[String]) -> Result<(), MigrationError> {
        for code in reasons {
            let added = sqlx::query("INSERT INTO migration_admitted_metadata_issue(import_id,plan_id,organization_id,code,count) VALUES($1,$2,$3,$4,1) ON CONFLICT DO NOTHING").bind(self.import).bind(self.plan).bind(self.org.0).bind(code).execute(&mut *c).await?.rows_affected();
            if added == 1 {
                sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_bytes=preparation_bytes+$2 WHERE id=$1").bind(self.plan).bind(code.len() as i64).execute(&mut *c).await?;
            } else {
                sqlx::query("UPDATE migration_admitted_metadata_issue SET count=count+1 WHERE plan_id=$1 AND organization_id=$2 AND code=$3").bind(self.plan).bind(self.org.0).bind(code).execute(&mut *c).await?;
            }
        }
        Ok(())
    }
    async fn sources(
        &self,
        c: &mut PgConnection,
        key: &RawPayloadKey,
        p: &PgRow,
    ) -> Result<(), MigrationError> {
        let seq: i64 = p.get("preparation_sequence");
        let ordinal: i32 = p.get("preparation_ordinal");
        let cap=sqlx::query("SELECT id,sequence,stream,accepted,truncated,http_status,classification,representation,raw_byte_len,nonce,CASE WHEN raw_byte_len<=4194304 AND octet_length(ciphertext)<=4194320 THEN ciphertext END AS ciphertext FROM migration_snapshot_capture WHERE snapshot_id=$1 AND organization_id=$2 AND stream IN ('people','custom_fields') AND sequence<=$3 AND sequence>=$4 AND (sequence>$4 OR sequence=$4 AND EXISTS(SELECT 1 FROM migration_snapshot_record r WHERE r.capture_id=migration_snapshot_capture.id AND r.organization_id=$2 AND r.ordinal>$5)) ORDER BY sequence LIMIT 1").bind(self.snapshot).bind(self.org.0).bind(self.boundary).bind(seq).bind(ordinal).fetch_optional(&mut *c).await?;
        let Some(cap) = cap else {
            return self.phase(c, "fields").await;
        };
        let capture: Uuid = cap.get("id");
        let sequence: i64 = cap.get("sequence");
        let family: String = cap.get("stream");
        let stream = Stream::parse(&family).ok_or(MigrationError::SourceNotEligible)?;
        let raw = match cap.get::<Option<Vec<u8>>, _>("ciphertext") {
            Some(ciphertext) => Some(
                crypto::open_snapshot(
                    key,
                    self.org,
                    self.snapshot,
                    capture,
                    "capture",
                    &cap.get::<Vec<u8>, _>("nonce"),
                    &ciphertext,
                )
                .map_err(|_| MigrationError::Crypto)?,
            ),
            None => None,
        };
        let parsed = raw
            .as_ref()
            .and_then(|raw| metadata_source::extract_page(stream, raw).ok());
        let page_reason = if raw.is_none() {
            "metadata_reader_byte_limit"
        } else {
            "source_representation_unavailable"
        };
        let rows=sqlx::query("SELECT * FROM migration_snapshot_record WHERE capture_id=$1 AND snapshot_id=$2 AND organization_id=$3 AND ordinal>$4 ORDER BY ordinal LIMIT 100").bind(capture).bind(self.snapshot).bind(self.org.0).bind(if sequence==seq{ordinal}else{-1}).fetch_all(&mut *c).await?;
        let mut last = -1;
        for r in rows {
            let index: i32 = r.get("ordinal");
            last = index;
            let parsed_record = parsed
                .as_ref()
                .and_then(|rows| {
                    usize::try_from(index)
                        .ok()
                        .and_then(|index| rows.get(index))
                })
                .cloned();
            let parsed_ok = parsed_record.is_some();
            let mut record = parsed_record.unwrap_or_else(|| Record {
                source_id: r.get("source_id"),
                canonical: vec![],
                entity: Entity::Invalid,
                reasons: vec![page_reason.into()],
                transformations: vec![],
                provenance: std::collections::BTreeMap::from([
                    (
                        "source_id".into(),
                        r.get::<Option<String>, _>("source_id").unwrap_or_default(),
                    ),
                    ("capture_id".into(), capture.to_string()),
                    ("snapshot_id".into(), self.snapshot.to_string()),
                    (
                        "raw_byte_len".into(),
                        cap.get::<i64, _>("raw_byte_len").to_string(),
                    ),
                    ("hold_reason".into(), page_reason.into()),
                ]),
            });
            let semantic = if parsed_ok {
                crypto::snapshot_hmac(
                    key,
                    self.org,
                    &format!("semantic:{}", stream.representation()),
                    &record.canonical,
                )
                .to_vec()
            } else {
                r.get::<Vec<u8>, _>("semantic_hmac")
            };
            if !parsed_ok {
                self.issues(c, &[page_reason.into()]).await?;
            }
            let source_id: Option<String> = r.get("source_id");
            let qualified = parsed_ok
                && cap.get::<bool, _>("accepted")
                && !cap.get::<bool, _>("truncated")
                && (200..300).contains(&cap.get::<i32, _>("http_status"))
                && cap.get::<String, _>("classification") == "success"
                && cap.get::<String, _>("representation") == stream.representation()
                && cap.get::<i64, _>("raw_byte_len") == raw.as_ref().map_or(0, |v| v.len()) as i64
                && r.get::<String, _>("family") == family
                && r.get::<String, _>("representation") == stream.representation()
                && record.source_id == source_id
                && r.get::<Vec<u8>, _>("semantic_hmac") == semantic;
            record.canonical.clear();
            let mut source_row = None;
            if let Some(source_id) = source_id {
                let existing=sqlx::query("SELECT id,semantic_hmac FROM migration_admitted_metadata_source WHERE plan_id=$1 AND organization_id=$2 AND family=$3 AND source_id=$4").bind(self.plan).bind(self.org.0).bind(&family).bind(&source_id).fetch_optional(&mut *c).await?;
                if let Some(old) = existing {
                    let id: Uuid = old.get("id");
                    source_row = Some(id);
                    sqlx::query("UPDATE migration_admitted_metadata_source SET qualified=qualified AND $3,conflict=conflict OR $4,observations=observations+1 WHERE id=$1 AND organization_id=$2").bind(id).bind(self.org.0).bind(qualified).bind(old.get::<Vec<u8>,_>("semantic_hmac")!=semantic).execute(&mut *c).await?;
                } else {
                    let id = Uuid::new_v4();
                    source_row = Some(id);
                    let frozen = FrozenSource {
                        report_id: self.report,
                        capture_id: capture,
                        capture_sequence: sequence,
                        ordinal: index,
                        qualified,
                        conflict: false,
                        record,
                    };
                    let encrypted = seal(
                        key,
                        self.org,
                        self.snapshot,
                        self.plan,
                        id,
                        "source",
                        &frozen,
                    )?;
                    sqlx::query("INSERT INTO migration_admitted_metadata_source(id,import_id,plan_id,organization_id,family,source_id,semantic_hmac,nonce,ciphertext,qualified,capture_id,capture_sequence,ordinal) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)").bind(id).bind(self.import).bind(self.plan).bind(self.org.0).bind(&family).bind(source_id).bind(semantic.as_slice()).bind(encrypted.nonce).bind(encrypted.ciphertext).bind(qualified).bind(capture).bind(sequence).bind(index).execute(&mut *c).await?;
                }
            }
            sqlx::query("INSERT INTO migration_admitted_metadata_observation(id,import_id,plan_id,organization_id,source_row_id,snapshot_record_id,semantic_hmac,qualified) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(Uuid::new_v4()).bind(self.import).bind(self.plan).bind(self.org.0).bind(source_row).bind(r.get::<Uuid,_>("id")).bind(semantic.as_slice()).bind(qualified).execute(&mut *c).await?;
        }
        // Empty pages still advance the capture key; no settled prefix is scanned.
        if last < 0 {
            last = i32::MAX;
        }
        sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_sequence=$2,preparation_ordinal=$3 WHERE id=$1").bind(self.plan).bind(sequence).bind(last).execute(c).await?;
        Ok(())
    }
    async fn phase(&self, c: &mut PgConnection, phase: &str) -> Result<(), MigrationError> {
        sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_phase=$2,preparation_key=NULL,preparation_parent=NULL,preparation_sequence=0,preparation_ordinal=-1 WHERE id=$1").bind(self.plan).bind(phase).execute(c).await?;
        Ok(())
    }
    async fn fields(
        &self,
        c: &mut PgConnection,
        key: &RawPayloadKey,
        p: &PgRow,
    ) -> Result<(), MigrationError> {
        let row=sqlx::query("SELECT id,source_id FROM migration_admitted_metadata_source WHERE plan_id=$1 AND organization_id=$2 AND family='custom_fields' AND id>$3 ORDER BY id LIMIT 1").bind(self.plan).bind(self.org.0).bind(p.get::<Option<Uuid>,_>("preparation_key").unwrap_or(Uuid::nil())).fetch_optional(&mut *c).await?;
        let Some(row) = row else {
            return self.phase(c, "tags").await;
        };
        let source_id: String = row.get("source_id");
        let source = insert_source(
            c,
            key,
            self.org,
            self.import,
            self.plan,
            self.snapshot,
            self.report,
            "custom_fields",
            &source_id,
        )
        .await?
        .ok_or(MigrationError::Crypto)?;
        let parent = if let Entity::Field(field) = source.record.entity {
            let mut reasons = field.reasons.clone();
            if !source.qualified || source.conflict {
                reasons.push("source_integrity".into());
            }
            Some(
                insert_mapping(
                    c,
                    key,
                    self.org,
                    self.snapshot,
                    self.account,
                    self.import,
                    self.plan,
                    "field",
                    &source_id,
                    source_id.as_bytes(),
                    None,
                    None,
                    FrozenMapping {
                        source_id: source_id.clone(),
                        label: field.label.clone(),
                        machine_name: field.name.clone(),
                        field_type: field.field_type.clone(),
                        raw_choice: None,
                        definition: Some(field),
                        target_baseline: Value::Null,
                        claim_baseline: Value::Null,
                        reasons,
                    },
                )
                .await?,
            )
        } else {
            None
        };
        sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_key=$2,preparation_parent=$3,preparation_ordinal=-1,preparation_phase=CASE WHEN $3::uuid IS NULL THEN 'fields' ELSE 'options' END,fields_processed=fields_processed+1 WHERE id=$1").bind(self.plan).bind(row.get::<Uuid,_>("id")).bind(parent).execute(c).await?;
        Ok(())
    }
    async fn options(
        &self,
        c: &mut PgConnection,
        key: &RawPayloadKey,
        p: &PgRow,
    ) -> Result<(), MigrationError> {
        let parent: Uuid = p
            .get::<Option<Uuid>, _>("preparation_parent")
            .ok_or(MigrationError::Crypto)?;
        let m=sqlx::query("SELECT * FROM migration_admitted_metadata_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(parent).bind(self.plan).bind(self.org.0).fetch_one(&mut *c).await?;
        let f: FrozenMapping = w::open(
            key,
            self.org,
            self.snapshot,
            self.plan,
            parent,
            "mapping",
            &m.get::<Vec<u8>, _>("nonce"),
            &m.get::<Vec<u8>, _>("ciphertext"),
        )?;
        let definition = f.definition.ok_or(MigrationError::Crypto)?;
        let after: i32 = p.get("preparation_ordinal");
        let choice = definition
            .choices
            .iter()
            .find(|v| i64::from(v.ordinal) > i64::from(after));
        if let Some(choice) = choice {
            if let Some(raw) = choice.raw.as_ref() {
                let material = serde_json::to_vec(&(f.source_id.as_str(), raw.as_str()))
                    .map_err(|_| MigrationError::Crypto)?;
                let mut reasons = choice.reasons.clone();
                if f.reasons.iter().any(|r| r == "source_integrity") {
                    reasons.push("source_integrity".into());
                }
                insert_mapping(
                    c,
                    key,
                    self.org,
                    self.snapshot,
                    self.account,
                    self.import,
                    self.plan,
                    "option",
                    &f.source_id,
                    &material,
                    Some(parent),
                    None,
                    FrozenMapping {
                        source_id: f.source_id.clone(),
                        label: choice.label.clone(),
                        machine_name: f.machine_name,
                        field_type: f.field_type,
                        raw_choice: Some(raw.clone()),
                        definition: None,
                        target_baseline: Value::Null,
                        claim_baseline: Value::Null,
                        reasons,
                    },
                )
                .await?;
            }
            sqlx::query(
                "UPDATE migration_admitted_metadata_plan SET preparation_ordinal=$2 WHERE id=$1",
            )
            .bind(self.plan)
            .bind(i32::try_from(choice.ordinal).map_err(|_| MigrationError::StorageLimit)?)
            .execute(c)
            .await?;
        } else {
            sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_phase='fields',preparation_parent=NULL,preparation_ordinal=-1 WHERE id=$1").bind(self.plan).execute(c).await?;
        }
        Ok(())
    }
    async fn tags(
        &self,
        conn: &mut PgConnection,
        key: &RawPayloadKey,
        p: &PgRow,
    ) -> Result<(), MigrationError> {
        let Self {
            org,
            import,
            plan,
            report,
            snapshot,
            account,
            admission,
            ..
        } = *self;
        let row=sqlx::query("SELECT s.* FROM migration_admitted_metadata_source s JOIN LATERAL(SELECT 1 FROM migration_people_admission_result r WHERE r.admission_id=$6 AND r.organization_id=s.organization_id AND r.source_id=s.source_id AND r.disposition='settled' LIMIT 1) cohort ON true WHERE s.plan_id=$1 AND s.organization_id=$2 AND s.family='people' AND s.qualified AND NOT s.conflict AND (s.capture_sequence,s.ordinal,s.id)>($3,$4,$5) ORDER BY s.capture_sequence,s.ordinal,s.id LIMIT 1").bind(plan).bind(org.0).bind(p.get::<i64,_>("preparation_sequence")).bind(p.get::<i32,_>("preparation_ordinal")).bind(p.get::<Option<Uuid>,_>("preparation_key").unwrap_or(Uuid::nil())).bind(admission).fetch_optional(&mut *conn).await?;
        let Some(row) = row else {
            return self.phase(conn, "choices").await;
        };
        let source_id: String = row.get("source_id");
        let source = insert_source(
            conn, key, org, import, plan, snapshot, report, "people", &source_id,
        )
        .await?
        .ok_or(MigrationError::Crypto)?;
        if source.record.reasons.is_empty() {
            if let Entity::Person(person) = source.record.entity {
                for tag in person.tags {
                    let Some(raw) = tag.raw else {
                        continue;
                    };
                    let label = tag.label.clone();
                    let material = if let Some(label) = &label {
                        label.as_bytes().to_vec()
                    } else {
                        raw.as_bytes().to_vec()
                    };
                    let mapping_id = insert_mapping(
                        conn,
                        key,
                        org,
                        snapshot,
                        account,
                        import,
                        plan,
                        "tag",
                        &source_id,
                        &material,
                        None,
                        None,
                        FrozenMapping {
                            source_id: source_id.clone(),
                            label,
                            machine_name: None,
                            field_type: None,
                            raw_choice: Some(raw.clone()),
                            definition: None,
                            target_baseline: Value::Null,
                            claim_baseline: Value::Null,
                            reasons: tag.reasons,
                        },
                    )
                    .await?;
                    sqlx::query("INSERT INTO migration_admitted_metadata_alias(id,import_id,plan_id,organization_id,mapping_id,source_row_id,ordinal) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(mapping_id,source_row_id,ordinal) DO NOTHING").bind(Uuid::new_v4()).bind(import).bind(plan).bind(org.0).bind(mapping_id).bind(row.get::<Uuid,_>("id")).bind(i32::try_from(tag.ordinal).map_err(|_|MigrationError::Crypto)?).execute(&mut *conn).await?;
                }
            }
        }
        sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_sequence=$2,preparation_ordinal=$3,preparation_key=$4 WHERE id=$1").bind(plan).bind(row.get::<i64,_>("capture_sequence")).bind(row.get::<i32,_>("ordinal")).bind(row.get::<Uuid,_>("id")).execute(conn).await?;
        Ok(())
    }
    async fn choices(
        &self,
        c: &mut PgConnection,
        key: &RawPayloadKey,
        p: &PgRow,
    ) -> Result<(), MigrationError> {
        use super::super::metadata::Choice;
        let row=sqlx::query("SELECT * FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND (kind,id)>($3,$4) ORDER BY kind,id LIMIT 1").bind(self.plan).bind(self.org.0).bind(p.get::<String,_>("preparation_kind")).bind(p.get::<Option<Uuid>,_>("preparation_key").unwrap_or(Uuid::nil())).fetch_optional(&mut *c).await?;
        let Some(m) = row else {
            return self.phase(c, "cohort").await;
        };
        let id: Uuid = m.get("id");
        let kind: String = m.get("kind");
        let source_key: Vec<u8> = m.get("source_key");
        let inputs: PlanChoices = w::open(
            key,
            self.org,
            self.snapshot,
            self.plan,
            self.plan,
            "inputs",
            &p.get::<Vec<u8>, _>("inputs_nonce"),
            &p.get::<Vec<u8>, _>("inputs_ciphertext"),
        )?;
        let explicit = inputs
            .patches
            .iter()
            .find(|v| v.kind == kind && v.source_key == source_key);
        let mut choice = Choice::Hold;
        if let Some(patch) = explicit {
            choice = patch.choice.clone();
        } else if let Some(previous) = p.get::<Option<Uuid>, _>("previous_plan_id") {
            let old=sqlx::query("SELECT disposition,target_id FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND source_key=$4").bind(previous).bind(self.org.0).bind(&kind).bind(&source_key).fetch_optional(&mut *c).await?;
            if let Some(old) = old {
                choice = match old.get::<String, _>("disposition").as_str() {
                    "create_matching" => Choice::CreateMatching,
                    "map_existing" => Choice::MapExisting {
                        target_id: old
                            .get::<Option<Uuid>, _>("target_id")
                            .ok_or(MigrationError::Crypto)?,
                    },
                    _ => Choice::Hold,
                };
            }
        }
        let mut frozen =
            super::fidelity::mapping(c, key, self.org, self.snapshot, self.plan, &m).await?;
        if let Some(name) = m.get::<Option<Vec<u8>>, _>("field_name_key") {
            let duplicate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND field_name_key=$3 AND id<>$4)").bind(self.plan).bind(self.org.0).bind(name).bind(id).fetch_one(&mut *c).await?;
            if duplicate {
                frozen.reasons.push("source_field_key_collision".into());
            }
        }
        let parent = if let Some(parent) = m.get::<Option<Uuid>, _>("parent_mapping_id") {
            sqlx::query_scalar::<_,Option<Uuid>>("SELECT target_id FROM migration_admitted_metadata_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3 AND disposition IN ('create_matching','map_existing')").bind(parent).bind(self.plan).bind(self.org.0).fetch_optional(&mut *c).await?.flatten()
        } else {
            None
        };
        let (mut disposition, mut target) = match choice {
            Choice::Hold => ("held", None),
            Choice::CreateMatching => ("create_matching", Some(Uuid::new_v4())),
            Choice::MapExisting { target_id } => ("map_existing", Some(target_id)),
        };
        if !frozen.reasons.is_empty() || kind == "option" && parent.is_none() {
            disposition = "held";
            target = None;
        }
        if let Some(target_id) = target {
            frozen.target_baseline = w::target_state(c, self.org, &kind, target_id).await?;
            frozen.claim_baseline =
                w::claim_state(c, self.org, self.account, &kind, &source_key).await?;
            let invalid = disposition == "map_existing"
                && (frozen.target_baseline.is_null()
                    || frozen.target_baseline["archived"] == true
                    || kind == "field"
                        && frozen.target_baseline["field_type"] != json!(frozen.field_type)
                    || kind == "option" && frozen.target_baseline["field_id"] != json!(parent))
                || disposition == "create_matching" && !frozen.target_baseline.is_null();
            let duplicate:bool=kind!="tag" && sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind=$3 AND target_id=$4 AND id<>$5)").bind(self.plan).bind(self.org.0).bind(&kind).bind(target_id).bind(id).fetch_one(&mut *c).await?;
            if invalid || kind != "tag" && duplicate {
                disposition = "held";
                target = None;
                frozen.reasons.push("invalid_destination".into());
            }
        }
        self.issues(c, &frozen.reasons).await?;
        // An option references its immutable parent definition; retaining the
        // full choice list in every option would multiply large source evidence.
        if kind == "option" {
            frozen.definition = None;
        }
        let encrypted = seal(
            key,
            self.org,
            self.snapshot,
            self.plan,
            id,
            "mapping",
            &frozen,
        )?;
        sqlx::query("UPDATE migration_admitted_metadata_mapping SET disposition=$3,target_id=$4,target_field_id=$5,nonce=$6,ciphertext=$7 WHERE id=$1 AND organization_id=$2").bind(id).bind(self.org.0).bind(disposition).bind(target).bind(parent).bind(encrypted.nonce).bind(encrypted.ciphertext).execute(&mut *c).await?;
        sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_kind=$2,preparation_key=$3 WHERE id=$1").bind(self.plan).bind(kind).bind(id).execute(c).await?;
        Ok(())
    }
    async fn cohort(
        &self,
        conn: &mut PgConnection,
        key: &RawPayloadKey,
        p: &PgRow,
    ) -> Result<(), MigrationError> {
        let Self {
            org,
            import,
            plan,
            report,
            snapshot,
            account,
            admission,
            ..
        } = *self;
        let result=sqlx::query("SELECT ar.id,ar.item_id,ar.person_id,ar.source_id,p.id AS live_person_id,COALESCE(identity.valid,false) AS identity_valid FROM migration_people_admission_result ar LEFT JOIN person p ON p.id=ar.person_id AND p.organization_id=ar.organization_id LEFT JOIN LATERAL(SELECT true AS valid FROM migration_import_identity mi WHERE mi.organization_id=ar.organization_id AND mi.source_account_id=$4 AND mi.family='people' AND mi.source_id=ar.source_id AND mi.target_id=ar.person_id AND mi.admission_id=ar.admission_id AND mi.admission_item_id=ar.item_id AND mi.admission_result_id=ar.id LIMIT 1) identity ON true WHERE ar.organization_id=$1 AND ar.admission_id=$2 AND ar.disposition='settled' AND ar.id>$3 ORDER BY ar.id LIMIT 1").bind(org.0).bind(admission).bind(p.get::<Option<Uuid>,_>("preparation_key").unwrap_or(Uuid::nil())).bind(account).fetch_optional(&mut *conn).await?;
        let Some(result) = result else {
            return self.phase(conn, "seal").await;
        };
        let result_id: Uuid = result.get("id");
        let person: Uuid = result
            .get::<Option<Uuid>, _>("person_id")
            .unwrap_or(Uuid::nil());
        let live: Option<Uuid> = result.get("live_person_id");
        let source_id: String = result.get("source_id");
        let source = insert_source(
            conn, key, org, import, plan, snapshot, report, "people", &source_id,
        )
        .await?;
        let manifest_id = Uuid::new_v4();
        let mut disposition = "held";
        let mut reasons = vec!["source_evidence_unavailable".to_owned()];
        let mut tag_operations: Vec<(Uuid, String)> = Vec::new();
        if let Some(source) = source {
            if source.qualified && !source.conflict && source.record.reasons.is_empty() {
                reasons.clear();
                if live.is_none() {
                    reasons.push("native_person_missing".into());
                } else if !result.get::<bool, _>("identity_valid") {
                    reasons.push("admission_identity_mismatch".into());
                } else {
                    disposition = "eligible";
                }
                if let Entity::Person(person_source) = source.record.entity {
                    for tag in person_source.tags {
                        let Some(raw) = tag.raw else {
                            continue;
                        };
                        let label = tag.label.clone();
                        let material = if let Some(label) = &label {
                            label.as_bytes().to_vec()
                        } else {
                            raw.as_bytes().to_vec()
                        };
                        let mapping_id = insert_mapping(
                            conn,
                            key,
                            org,
                            snapshot,
                            account,
                            import,
                            plan,
                            "tag",
                            &source_id,
                            &material,
                            None,
                            None,
                            FrozenMapping {
                                source_id: source_id.clone(),
                                label,
                                machine_name: None,
                                field_type: None,
                                raw_choice: Some(raw.clone()),
                                definition: None,
                                target_baseline: Value::Null,
                                claim_baseline: Value::Null,
                                reasons: tag.reasons,
                            },
                        )
                        .await?;
                        tag_operations.push((mapping_id, raw));
                    }
                } else {
                    disposition = "held";
                    reasons = vec!["source_representation_mismatch".into()];
                }
            } else {
                reasons = vec!["source_integrity".into()];
            }
        }
        let (baseline, oversized) = baseline(
            conn,
            key,
            org,
            snapshot,
            plan,
            manifest_id,
            person,
            &mut reasons,
        )
        .await?;
        if oversized {
            disposition = "held";
        }
        self.issues(conn, &reasons).await?;
        let bound = (baseline.nonce.len() + baseline.ciphertext.len() + 256 * 1024) as i64;
        if bound > 64 * 1024 * 1024 {
            return Err(MigrationError::StorageLimit);
        }
        sqlx::query("INSERT INTO migration_admitted_metadata_manifest(id,import_id,plan_id,organization_id,admission_result_id,person_id,source_person_id,disposition,baseline_nonce,baseline_ciphertext,item_byte_bound,expected_person_id,oversized) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
            .bind(manifest_id).bind(import).bind(plan).bind(org.0).bind(result_id).bind(live).bind(&source_id).bind(disposition).bind(baseline.nonce).bind(baseline.ciphertext).bind(bound).bind(if person.is_nil(){None}else{Some(person)}).bind(oversized).execute(&mut *conn).await?;
        for (mapping_id, raw) in tag_operations {
            let op_id = Uuid::new_v4();
            let op = FrozenOperation {
                source_id: source_id.clone(),
                source_field: None,
                source_tag: Some(raw),
                value: None,
                reasons: reasons.clone(),
            };
            let sealed = seal(key, org, snapshot, plan, op_id, "operation", &op)?;
            let op_key: Vec<u8> = sqlx::query_scalar("SELECT source_key FROM migration_admitted_metadata_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3")
                .bind(mapping_id).bind(plan).bind(org.0).fetch_one(&mut *conn).await?;
            sqlx::query("INSERT INTO migration_admitted_metadata_operation(id,manifest_id,import_id,plan_id,organization_id,kind,mapping_id,source_key,disposition,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,'tag_link',$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING")
                .bind(op_id).bind(manifest_id).bind(import).bind(plan).bind(org.0).bind(mapping_id).bind(op_key).bind(if disposition=="eligible" {"eligible"} else {"held"}).bind(sealed.nonce).bind(sealed.ciphertext).execute(&mut *conn).await?;
        }

        sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_key=$2,preparation_manifest=$3,preparation_parent=NULL,preparation_phase='values',people_processed=people_processed+1 WHERE id=$1").bind(plan).bind(result_id).bind(manifest_id).execute(&mut *conn).await?;
        let updated = sqlx::query("SELECT * FROM migration_admitted_metadata_plan WHERE id=$1")
            .bind(plan)
            .fetch_one(&mut *conn)
            .await?;
        self.values(conn, key, &updated).await
    }
    async fn values(
        &self,
        conn: &mut PgConnection,
        key: &RawPayloadKey,
        p: &PgRow,
    ) -> Result<(), MigrationError> {
        let Self {
            org,
            import,
            plan,
            report,
            snapshot,
            ..
        } = *self;
        let manifest_id: Uuid = p
            .get::<Option<Uuid>, _>("preparation_manifest")
            .ok_or(MigrationError::Crypto)?;
        let m=sqlx::query("SELECT * FROM migration_admitted_metadata_manifest WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(manifest_id).bind(plan).bind(org.0).fetch_one(&mut *conn).await?;
        let source_id: String = m.get("source_person_id");
        let disposition: String = m.get("disposition");
        let source = insert_source(
            conn, key, org, import, plan, snapshot, report, "people", &source_id,
        )
        .await?;
        let mut last = p.get::<Option<Uuid>, _>("preparation_parent");
        let mut complete = false;
        for _ in 0..50 {
            let row=sqlx::query("SELECT * FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND kind='field' AND id>$3 ORDER BY id LIMIT 1").bind(plan).bind(org.0).bind(last.unwrap_or(Uuid::nil())).fetch_optional(&mut *conn).await?;
            let Some(row) = row else {
                complete = true;
                break;
            };
            let mapping_id: Uuid = row.get("id");
            last = Some(mapping_id);
            if let Some(ref source) = source {
                let record = &source.record;
                let f: FrozenMapping = w::open(
                    key,
                    org,
                    snapshot,
                    plan,
                    mapping_id,
                    "mapping",
                    &row.get::<Vec<u8>, _>("nonce"),
                    &row.get::<Vec<u8>, _>("ciphertext"),
                )?;
                if let Some(field) = f.definition {
                    let field_specs = vec![(field, mapping_id, f.source_id)];
                    for (field, mapping_id, field_source_id) in &field_specs {
                        let extracted = metadata_source::extract_value(record, field);
                        let op_id = Uuid::new_v4();
                        let op = FrozenOperation {
                            source_id: source_id.clone(),
                            source_field: Some(field_source_id.clone()),
                            source_tag: None,
                            value: extracted.value.clone(),
                            reasons: extracted.reasons.clone(),
                        };
                        let sealed = seal(key, org, snapshot, plan, op_id, "operation", &op)?;
                        let op_key: Vec<u8> = sqlx::query_scalar("SELECT source_key FROM migration_admitted_metadata_mapping WHERE id=$1 AND plan_id=$2 AND organization_id=$3")
                    .bind(*mapping_id).bind(plan).bind(org.0).fetch_one(&mut *conn).await?;
                        let operation_disposition = if disposition == "eligible" {
                            extracted.disposition
                        } else {
                            "held".to_owned()
                        };
                        sqlx::query("INSERT INTO migration_admitted_metadata_operation(id,manifest_id,import_id,plan_id,organization_id,kind,mapping_id,source_key,disposition,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,'value',$6,$7,$8,$9,$10)")
                    .bind(op_id).bind(manifest_id).bind(import).bind(plan).bind(org.0).bind(*mapping_id).bind(op_key).bind(operation_disposition).bind(sealed.nonce).bind(sealed.ciphertext).execute(&mut *conn).await?;
                    }
                }
            }
        }
        if complete {
            sqlx::query("UPDATE migration_admitted_metadata_operation o SET target_id=m.target_id,disposition=CASE WHEN o.disposition='eligible' AND m.disposition='held' THEN 'held' ELSE o.disposition END FROM migration_admitted_metadata_mapping m WHERE o.manifest_id=$1 AND o.organization_id=$2 AND m.id=o.mapping_id AND m.plan_id=o.plan_id AND m.organization_id=o.organization_id").bind(manifest_id).bind(org.0).execute(&mut *conn).await?;
        }
        sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_parent=$2,preparation_phase=CASE WHEN $3 THEN 'extra_values' ELSE 'values' END,preparation_ordinal=CASE WHEN $3 THEN 0 ELSE preparation_ordinal END WHERE id=$1").bind(plan).bind(last).bind(complete).execute(conn).await?;
        Ok(())
    }
    async fn extra_values(
        &self,
        c: &mut PgConnection,
        key: &RawPayloadKey,
        p: &PgRow,
    ) -> Result<(), MigrationError> {
        let manifest: Uuid = p.get("preparation_manifest");
        let m=sqlx::query("SELECT * FROM migration_admitted_metadata_manifest WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(manifest).bind(self.plan).bind(self.org.0).fetch_one(&mut *c).await?;
        let source = insert_source(
            c,
            key,
            self.org,
            self.import,
            self.plan,
            self.snapshot,
            self.report,
            "people",
            &m.get::<String, _>("source_person_id"),
        )
        .await?;
        let offset: usize = p
            .get::<i32, _>("preparation_ordinal")
            .try_into()
            .map_err(|_| MigrationError::Crypto)?;
        let mut count = 0;
        if let Some(source) = source {
            for (name, _) in source.record.provenance.iter().skip(offset).take(50) {
                count += 1;
                if !name.starts_with("custom") {
                    continue;
                }
                let name_key = metadata_store::source_key(
                    key,
                    self.org,
                    self.account,
                    "field-name",
                    name.as_bytes(),
                );
                let declared:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND field_name_key=$3)").bind(self.plan).bind(self.org.0).bind(name_key).fetch_one(&mut *c).await?;
                if declared {
                    continue;
                }
                let id = Uuid::new_v4();
                let op = FrozenOperation {
                    source_id: m.get("source_person_id"),
                    source_field: Some(name.clone()),
                    source_tag: None,
                    value: None,
                    reasons: vec!["source_definition_unavailable".into()],
                };
                let encrypted = seal(
                    key,
                    self.org,
                    self.snapshot,
                    self.plan,
                    id,
                    "operation",
                    &op,
                )?;
                let source_key = metadata_store::source_key(
                    key,
                    self.org,
                    self.account,
                    "unresolved-field",
                    name.as_bytes(),
                );
                sqlx::query("INSERT INTO migration_admitted_metadata_operation(id,manifest_id,import_id,plan_id,organization_id,kind,source_key,disposition,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,'value',$6,'held',$7,$8)").bind(id).bind(manifest).bind(self.import).bind(self.plan).bind(self.org.0).bind(source_key).bind(encrypted.nonce).bind(encrypted.ciphertext).execute(&mut *c).await?;
                self.issues(c, &op.reasons).await?;
            }
        }
        if count == 50 {
            sqlx::query(
                "UPDATE migration_admitted_metadata_plan SET preparation_ordinal=$2 WHERE id=$1",
            )
            .bind(self.plan)
            .bind(i32::try_from(offset + count).map_err(|_| MigrationError::Crypto)?)
            .execute(c)
            .await?;
        } else {
            self.person_bound(c, key, &m).await?;
            sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_phase='cohort',preparation_ordinal=-1 WHERE id=$1").bind(self.plan).execute(c).await?;
        }
        Ok(())
    }
    async fn person_bound(
        &self,
        c: &mut PgConnection,
        key: &RawPayloadKey,
        m: &PgRow,
    ) -> Result<(), MigrationError> {
        let id: Uuid = m.get("id");
        let bound:i64=sqlx::query_scalar("SELECT COALESCE(sum(3*(octet_length(nonce)+octet_length(ciphertext))+512),0)::bigint FROM migration_admitted_metadata_operation WHERE manifest_id=$1 AND plan_id=$2 AND organization_id=$3").bind(id).bind(self.plan).bind(self.org.0).fetch_one(&mut *c).await?;
        let bound = bound
            + (m.get::<Vec<u8>, _>("baseline_nonce").len()
                + m.get::<Vec<u8>, _>("baseline_ciphertext").len()) as i64
            + 256 * 1024;
        let oversized = m.get::<bool, _>("oversized") || bound > metadata_store::UNIT;
        if oversized {
            let mut baseline: Value = w::open(
                key,
                self.org,
                self.snapshot,
                self.plan,
                id,
                "baseline",
                &m.get::<Vec<u8>, _>("baseline_nonce"),
                &m.get::<Vec<u8>, _>("baseline_ciphertext"),
            )?;
            let reasons = baseline["reasons"]
                .as_array_mut()
                .ok_or(MigrationError::Crypto)?;
            if !reasons.iter().any(|v| v == "import_item_byte_limit") {
                reasons.push(json!("import_item_byte_limit"));
                self.issues(c, &["import_item_byte_limit".into()]).await?;
            }
            let encrypted = seal(
                key,
                self.org,
                self.snapshot,
                self.plan,
                id,
                "baseline",
                &baseline,
            )?;
            sqlx::query("UPDATE migration_admitted_metadata_manifest SET oversized=true,disposition='held',baseline_nonce=$4,baseline_ciphertext=$5,item_byte_bound=262144 WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(id).bind(self.plan).bind(self.org.0).bind(encrypted.nonce).bind(encrypted.ciphertext).execute(&mut *c).await?;
            sqlx::query("UPDATE migration_admitted_metadata_operation SET disposition='held' WHERE manifest_id=$1 AND plan_id=$2 AND organization_id=$3 AND disposition IN ('eligible','already_present')").bind(id).bind(self.plan).bind(self.org.0).execute(c).await?;
        } else {
            sqlx::query("UPDATE migration_admitted_metadata_manifest SET item_byte_bound=$4 WHERE id=$1 AND plan_id=$2 AND organization_id=$3").bind(id).bind(self.plan).bind(self.org.0).bind(bound).execute(c).await?;
        }
        Ok(())
    }
    async fn finish(
        &self,
        c: &mut PgConnection,
        key: &RawPayloadKey,
    ) -> Result<(), MigrationError> {
        use super::super::metadata_model::Counts;
        let mut counts = Counts::default();
        let people=sqlx::query("SELECT count(*) AS source,count(*) FILTER(WHERE disposition='eligible') AS eligible,count(*) FILTER(WHERE disposition='held') AS excluded FROM migration_admitted_metadata_manifest WHERE plan_id=$1 AND organization_id=$2").bind(self.plan).bind(self.org.0).fetch_one(&mut *c).await?;
        counts.people.source = people.get("source");
        counts.people.eligible = people.get("eligible");
        counts.people.excluded = people.get("excluded");
        let groups=sqlx::query("SELECT kind,CASE WHEN disposition IN ('create_matching','map_existing') THEN 'eligible' ELSE disposition END AS disposition,count(*) AS n FROM migration_admitted_metadata_mapping WHERE plan_id=$1 AND organization_id=$2 AND execute_unit GROUP BY kind,2 UNION ALL SELECT kind,disposition,count(*) AS n FROM migration_admitted_metadata_operation WHERE plan_id=$1 AND organization_id=$2 GROUP BY kind,disposition").bind(self.plan).bind(self.org.0).fetch_all(&mut *c).await?;
        for row in groups {
            let kind: String = row.get("kind");
            let disposition: String = row.get("disposition");
            let n: i64 = row.get("n");
            let family = counts.family(&kind);
            family.planned += n;
            family.pending += n;
            family.add(&disposition, n);
            if disposition == "held" {
                counts.held_count += n;
            }
        }
        counts.invalid_source_ids=sqlx::query_scalar("SELECT count(*) FROM migration_admitted_metadata_observation WHERE plan_id=$1 AND organization_id=$2 AND source_row_id IS NULL").bind(self.plan).bind(self.org.0).fetch_one(&mut *c).await?;
        let value = serde_json::to_value(&counts).map_err(|_| MigrationError::Crypto)?;
        let bound:i64=sqlx::query_scalar("SELECT GREATEST(4096,COALESCE((SELECT max(item_byte_bound) FROM migration_admitted_metadata_manifest WHERE plan_id=$1 AND organization_id=$2),0),COALESCE((SELECT max(octet_length(m.nonce)+octet_length(m.ciphertext)+4096+CASE WHEN m.kind='field' AND m.disposition='create_matching' THEN COALESCE((SELECT sum(octet_length(o.nonce)+octet_length(o.ciphertext)+4096) FROM migration_admitted_metadata_mapping o WHERE o.parent_mapping_id=m.id AND o.plan_id=m.plan_id AND o.organization_id=m.organization_id),0) ELSE 0 END) FROM migration_admitted_metadata_mapping m WHERE m.plan_id=$1 AND m.organization_id=$2),0))::bigint").bind(self.plan).bind(self.org.0).fetch_one(&mut *c).await?;
        if bound > metadata_store::UNIT {
            return Err(MigrationError::StorageLimit);
        }
        let binding:Value=sqlx::query_scalar("SELECT jsonb_build_object('root',i.id,'plan',p.id,'revision',p.revision::text,'workspace_revision',i.workspace_revision::text,'admission',i.admission_id,'admission_plan',i.admission_plan_id,'snapshot',p.snapshot_id,'capture_sequence',p.capture_sequence::text,'report',p.source_report_id,'output_revision',p.source_output_revision,'inputs',encode(p.inputs_ciphertext,'hex')) FROM migration_admitted_metadata_import i JOIN migration_admitted_metadata_plan p ON p.import_id=i.id AND p.organization_id=i.organization_id WHERE i.id=$1 AND p.id=$2 AND i.organization_id=$3").bind(self.import).bind(self.plan).bind(self.org.0).fetch_one(&mut *c).await?;
        let digest = crypto::request_digest(
            key,
            "admitted-metadata-confirmation-v1",
            &serde_json::to_vec(&(binding, &value, bound)).map_err(|_| MigrationError::Crypto)?,
        );
        let before:i32=sqlx::query_scalar("SELECT COALESCE(octet_length(counts::text),0) FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2").bind(self.import).bind(self.org.0).fetch_one(&mut *c).await?;
        sqlx::query("UPDATE migration_admitted_metadata_import SET counts=$3 WHERE id=$1 AND organization_id=$2").bind(self.import).bind(self.org.0).bind(&value).execute(&mut *c).await?;
        sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_bytes=preparation_bytes+2*octet_length($2::jsonb::text)-octet_length(counts::text)-$3+32-COALESCE(octet_length(digest),0),counts=$2,digest=$4,max_added_byte_bound=$5,preparation_phase='complete',state='ready',expires_at=clock_timestamp()+interval '10 minutes' WHERE id=$1").bind(self.plan).bind(value).bind(before).bind(digest.as_slice()).bind(bound).execute(c).await?;
        Ok(())
    }
}
async fn unit(
    pool: &PgPool,
    key: &RawPayloadKey,
    candidate: &PgRow,
) -> Result<bool, MigrationError> {
    let org = OrganizationId::new(candidate.get("organization_id"));
    let import: Uuid = candidate.get("id");
    let ctx = CommandContext {
        organization_id: org,
        actor_user_id: UserId::new(candidate.get("executor_user_id")),
        origin: Origin::Migration,
        correlation_id: CorrelationId::new(import),
    };
    let mut tx = lifecycle_tx(pool, &ctx).await?;
    sqlx::query("SELECT s.id FROM migration_snapshot s JOIN migration_snapshot_storage l ON l.organization_id=s.organization_id WHERE s.id=$1 AND s.organization_id=$2 FOR UPDATE OF s,l").bind(candidate.get::<Uuid,_>("snapshot_id")).bind(org.0).fetch_one(&mut *tx).await?;
    let r=sqlx::query("SELECT * FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(import).bind(org.0).fetch_one(&mut *tx).await?;
    let is_remainder = r.get::<Option<Uuid>, _>("predecessor_import_id").is_some()
        && r.get::<String, _>("phase") == "preparation";
    if (!is_remainder
        && (r.get::<String, _>("state") != "proposed"
            || r.get::<Option<Uuid>, _>("confirmed_plan_id").is_some()))
        || (is_remainder && r.get::<String, _>("state") != "queued")
        || r.get::<Uuid, _>("executor_user_id") != ctx.actor_user_id.0
    {
        return Ok(false);
    }
    let plan: Uuid = r
        .get::<Option<Uuid>, _>("latest_plan_id")
        .ok_or(MigrationError::Crypto)?;
    let p=sqlx::query("SELECT * FROM migration_admitted_metadata_plan WHERE id=$1 AND import_id=$2 AND organization_id=$3 FOR UPDATE").bind(plan).bind(import).bind(org.0).fetch_one(&mut *tx).await?;
    if p.get::<String, _>("state") != "building" {
        return Ok(false);
    }
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organization o JOIN migration_workspace w ON w.organization_id=o.id JOIN migration_import i ON i.id=w.import_id AND i.organization_id=o.id AND i.confirmed_plan_id=w.plan_id JOIN migration_people_admission a ON a.id=$4 AND a.organization_id=o.id JOIN migration_core_change_report cr ON cr.id=$5 AND cr.organization_id=o.id WHERE o.id=$1 AND o.workspace_mode='migration_review' AND o.workspace_revision=$2 AND w.import_id=$3 AND i.state='completed' AND a.state IN ('completed','cancelled') AND cr.state='completed' AND cr.output_revision=$6 AND cr.newer_snapshot_id=$7 AND cr.newer_sequence=$8)").bind(org.0).bind(r.get::<i64,_>("workspace_revision")).bind(r.get::<Uuid,_>("parent_import_id")).bind(r.get::<Uuid,_>("admission_id")).bind(r.get::<Uuid,_>("source_report_id")).bind(p.get::<Uuid,_>("source_output_revision")).bind(p.get::<Uuid,_>("snapshot_id")).bind(p.get::<i64,_>("capture_sequence")).fetch_one(&mut *tx).await?;
    if !valid {
        return Err(MigrationError::SourceNotEligible);
    }
    let j = Prep {
        org,
        import,
        plan,
        report: r.get("source_report_id"),
        snapshot: r.get("snapshot_id"),
        account: r.get("source_account_id"),
        admission: r.get("admission_id"),
        boundary: r.get("capture_sequence"),
    };
    let lease = Uuid::new_v4();
    sqlx::query("UPDATE migration_admitted_metadata_import SET lease_token=$2,lease_expires_at=clock_timestamp()+interval '60 seconds' WHERE id=$1").bind(import).bind(lease).execute(&mut *tx).await?;
    let reservation = w::reserve(
        &mut tx,
        org,
        import,
        plan,
        j.snapshot,
        "prepare",
        64 * 1024 * 1024,
    )
    .await?;
    match p.get::<String, _>("preparation_phase").as_str() {
        "sources" => j.sources(&mut tx, key, &p).await?,
        "fields" => j.fields(&mut tx, key, &p).await?,
        "options" => j.options(&mut tx, key, &p).await?,
        "tags" => j.tags(&mut tx, key, &p).await?,
        "choices" => j.choices(&mut tx, key, &p).await?,
        "cohort" => j.cohort(&mut tx, key, &p).await?,
        "values" => j.values(&mut tx, key, &p).await?,
        "extra_values" => j.extra_values(&mut tx, key, &p).await?,
        "seal" => j.finish(&mut tx, key).await?,
        "remainder_mappings" | "remainder_people" | "remainder_operations" => {
            super::remainder::step(&mut tx, key, org, import, plan, &p).await?
        }
        _ => return Err(MigrationError::Conflict),
    }
    let actual: i64 = sqlx::query_scalar(
        "SELECT preparation_bytes FROM migration_admitted_metadata_plan WHERE id=$1",
    )
    .bind(plan)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE migration_admitted_metadata_plan SET preparation_bytes=0 WHERE id=$1")
        .bind(plan)
        .execute(&mut *tx)
        .await?;
    w::settle(&mut tx, org, import, reservation, actual).await?;
    if is_remainder && p.get::<String, _>("preparation_phase") == "seal" {
        sqlx::query("UPDATE migration_admitted_metadata_import SET phase='catalog',checkpoint_id=NULL,settled_eligible_people=0,held_settled_people=0 WHERE id=$1 AND organization_id=$2").bind(import).bind(org.0).execute(&mut *tx).await?;
    }
    let changed=sqlx::query("UPDATE migration_admitted_metadata_import SET lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 AND organization_id=$2 AND lease_token=$3 AND lease_expires_at>clock_timestamp()").bind(import).bind(org.0).bind(lease).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(MigrationError::ImportBusy);
    }
    tx.commit().await?;
    Ok(true)
}
pub(crate) async fn run_once(pool: &PgPool, key: &RawPayloadKey) -> Result<bool, MigrationError> {
    let candidate=sqlx::query("SELECT i.* FROM migration_admitted_metadata_import i JOIN migration_admitted_metadata_plan p ON p.id=i.latest_plan_id AND p.organization_id=i.organization_id WHERE (i.state='proposed' OR i.state='queued' AND p.remainder_plan_id IS NOT NULL) AND p.state='building' AND (i.lease_expires_at IS NULL OR i.lease_expires_at<=clock_timestamp()) ORDER BY i.created_at,i.id LIMIT 1").fetch_optional(pool).await?;
    let Some(candidate) = candidate else {
        return Ok(false);
    };
    match unit(pool, key, &candidate).await {
        Ok(v) => Ok(v),
        Err(error) => {
            let reason = match error {
                MigrationError::Crypto => "key_or_integrity",
                MigrationError::StorageLimit => "storage_limit",
                MigrationError::Forbidden => "authority",
                _ => "preparation_failed",
            };
            sqlx::query("UPDATE migration_admitted_metadata_import SET state='paused',pause_reason=$3,lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2 AND state IN ('proposed','queued') AND phase='preparation' AND executor_user_id=$4 AND latest_plan_id=$5").bind(candidate.get::<Uuid,_>("id")).bind(candidate.get::<Uuid,_>("organization_id")).bind(reason).bind(candidate.get::<Uuid,_>("executor_user_id")).bind(candidate.get::<Option<Uuid>,_>("latest_plan_id")).execute(pool).await?;
            Err(error)
        }
    }
}
