//! Fenced retained-only admitted metadata execution.
use super::{crypto, MigrationError};
use crate::{config::RawPayloadKey, ids::OrganizationId};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

async fn permit(
    c: &mut sqlx::PgConnection,
    r: Uuid,
    p: Uuid,
    l: Uuid,
    u: Uuid,
    t: Uuid,
    k: &str,
    s: &[u8],
) -> Result<(), MigrationError> {
    let claim_key = s
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let proof = json!({"lease":l,"unit_id":u,"root_id":r,"plan_id":p,"target_id":t,"claim_kind":k,"claim_key":claim_key});
    sqlx::query("SELECT set_config('crm.admitted_metadata_permit',$1,true)")
        .bind(serde_json::to_string(&proof).map_err(|_| MigrationError::Crypto)?)
        .execute(c)
        .await?;
    Ok(())
}
pub async fn run_once(pool: &PgPool, key: &RawPayloadKey) -> Result<bool, MigrationError> {
    let c=sqlx::query("SELECT id,organization_id FROM migration_admitted_metadata_import WHERE state='queued' ORDER BY created_at,id LIMIT 1").fetch_optional(pool).await?;
    let Some(c) = c else { return Ok(false) };
    let root: Uuid = c.get("id");
    let org = OrganizationId::new(c.get("organization_id"));
    let mut tx = pool.begin().await?;
    crate::auth::workspace::bounded_lock_wait(&mut tx).await?;
    super::store::lock_org(&mut tx, org).await?;
    let r=sqlx::query("SELECT * FROM migration_admitted_metadata_import WHERE id=$1 AND organization_id=$2 FOR UPDATE").bind(root).bind(org.0).fetch_one(&mut *tx).await?;
    if r.get::<String, _>("state") != "queued" {
        tx.commit().await?;
        return Ok(false);
    }
    let plan: Uuid = r
        .get::<Option<Uuid>, _>("confirmed_plan_id")
        .ok_or(MigrationError::Conflict)?;
    let snapshot: Uuid = r.get("snapshot_id");
    let actor: Uuid = r.get("executor_user_id");
    let account: i64 = r.get("source_account_id");
    let lease = Uuid::new_v4();
    sqlx::query("UPDATE migration_admitted_metadata_import SET state='running',lease_token=$3,lease_expires_at=clock_timestamp()+interval '60 seconds',phase='catalog' WHERE id=$1 AND organization_id=$2").bind(root).bind(org.0).bind(lease).execute(&mut *tx).await?;
    // Claim-backed catalog creation.  The permit binds the exact mapping and
    // target; a forged or stale lease reaches the workspace trigger and rolls back.
    let maps=sqlx::query("SELECT * FROM migration_admitted_metadata_mapping WHERE import_id=$1 AND plan_id=$2 AND organization_id=$3 AND kind IN ('tag','field') AND disposition='create_matching' ORDER BY id FOR UPDATE").bind(root).bind(plan).bind(org.0).fetch_all(&mut *tx).await?;
    for m in maps {
        let id: Uuid = m.get("id");
        let target: Uuid = m
            .get::<Option<Uuid>, _>("target_id")
            .ok_or(MigrationError::Conflict)?;
        let raw = crypto::open_snapshot(
            key,
            org,
            snapshot,
            id,
            &format!("admitted-metadata-v1:{plan}:mapping"),
            &m.get::<Vec<u8>, _>("nonce"),
            &m.get::<Vec<u8>, _>("ciphertext"),
        )
        .map_err(|_| MigrationError::Crypto)?;
        let v: serde_json::Value =
            serde_json::from_slice(&raw).map_err(|_| MigrationError::Crypto)?;
        let label = v["label"]
            .as_str()
            .ok_or(MigrationError::InvalidImportChoice)?;
        let source_key: Vec<u8> = m.get("source_key");
        let kind: String = m.get("kind");
        permit(&mut tx, root, plan, lease, id, target, &kind, &source_key).await?;
        if kind == "tag" {
            sqlx::query(
                "INSERT INTO tag(id,organization_id,created_by_user_id,name) VALUES($1,$2,$3,$4)",
            )
            .bind(target)
            .bind(org.0)
            .bind(actor)
            .bind(label)
            .execute(&mut *tx)
            .await?;
        } else {
            let ty = v["field_type"]
                .as_str()
                .ok_or(MigrationError::InvalidImportChoice)?;
            sqlx::query("INSERT INTO custom_field(id,organization_id,label,field_type,position,created_by_user_id,source,external_key) VALUES($1,$2,$3,$4,(SELECT COALESCE(max(position),-1)+1 FROM custom_field WHERE organization_id=$2),$5,'fub',$6)").bind(target).bind(org.0).bind(label).bind(ty).bind(actor).bind(v["source_id"].as_str()).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO migration_metadata_catalog_claim(organization_id,source_account_id,kind,source_key,target_id,admitted_import_id,admitted_plan_id,admitted_mapping_id,evidence_nonce,evidence_ciphertext) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(org.0).bind(account).bind(kind).bind(source_key).bind(target).bind(root).bind(plan).bind(id).bind(m.get::<Vec<u8>,_>("nonce")).bind(m.get::<Vec<u8>,_>("ciphertext")).execute(&mut *tx).await?;
    }
    let rows=sqlx::query("SELECT m.id,m.person_id,o.id AS op_id,o.mapping_id,o.source_key,o.target_id,o.disposition FROM migration_admitted_metadata_manifest m JOIN migration_admitted_metadata_operation o ON o.manifest_id=m.id AND o.organization_id=m.organization_id WHERE m.import_id=$1 AND m.plan_id=$2 AND m.organization_id=$3 AND m.disposition='eligible' AND o.kind='tag_link' ORDER BY m.id,o.id FOR UPDATE OF m,o").bind(root).bind(plan).bind(org.0).fetch_all(&mut *tx).await?;
    for x in rows {
        let manifest: Uuid = x.get("id");
        let person: Uuid = x.get("person_id");
        let outcome = if x.get::<String, _>("disposition") != "eligible" {
            "held"
        } else {
            let _map: Uuid = x
                .get::<Option<Uuid>, _>("mapping_id")
                .ok_or(MigrationError::Conflict)?;
            let target: Uuid = x
                .get::<Option<Uuid>, _>("target_id")
                .ok_or(MigrationError::Conflict)?;
            let sk: Vec<u8> = x.get("source_key");
            let claim:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_metadata_catalog_claim WHERE organization_id=$1 AND source_account_id=$2 AND kind='tag' AND source_key=$3 AND target_id=$4)").bind(org.0).bind(account).bind(&sk).bind(target).fetch_one(&mut *tx).await?;
            if !claim {
                "held"
            } else {
                permit(&mut tx, root, plan, lease, manifest, target, "tag", &sk).await?;
                let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM person_tag WHERE organization_id=$1 AND person_id=$2 AND tag_id=$3)").bind(org.0).bind(person).bind(target).fetch_one(&mut *tx).await?;
                if exists {
                    "already_present"
                } else {
                    sqlx::query("INSERT INTO person_tag(organization_id,person_id,tag_id,added_by_user_id) VALUES($1,$2,$3,$4)").bind(org.0).bind(person).bind(target).bind(actor).execute(&mut *tx).await?;
                    "applied"
                }
            }
        };
        let id = Uuid::new_v4();
        let raw =
            serde_json::to_vec(&json!({"outcome":outcome})).map_err(|_| MigrationError::Crypto)?;
        let sealed = crypto::seal_snapshot(
            key,
            org,
            snapshot,
            id,
            &format!("admitted-metadata-v1:{plan}:result"),
            &raw,
        )
        .map_err(|_| MigrationError::Crypto)?;
        sqlx::query("INSERT INTO migration_admitted_metadata_result(id,import_id,plan_id,manifest_id,organization_id,kind,disposition,person_id,nonce,ciphertext) VALUES($1,$2,$3,$4,$5,'people',$6,$7,$8,$9) ON CONFLICT(import_id,organization_id,kind,manifest_id) DO NOTHING").bind(id).bind(root).bind(plan).bind(manifest).bind(org.0).bind(outcome).bind(person).bind(sealed.nonce).bind(sealed.ciphertext).execute(&mut *tx).await?;
        sqlx::query("UPDATE migration_admitted_metadata_manifest SET disposition='settled',settled_at=clock_timestamp() WHERE id=$1 AND organization_id=$2").bind(manifest).bind(org.0).execute(&mut *tx).await?;
    }
    // Values are insert-only: an equal frozen text value is already present;
    // a differing value is held and never overwritten or adopted.
    let values=sqlx::query("SELECT m.id,m.person_id,o.id AS op_id,o.mapping_id,o.source_key,o.target_id,o.disposition,o.nonce,o.ciphertext FROM migration_admitted_metadata_manifest m JOIN migration_admitted_metadata_operation o ON o.manifest_id=m.id AND o.organization_id=m.organization_id WHERE m.import_id=$1 AND m.plan_id=$2 AND m.organization_id=$3 AND m.disposition='eligible' AND o.kind='value' ORDER BY m.id,o.id FOR UPDATE OF m,o").bind(root).bind(plan).bind(org.0).fetch_all(&mut *tx).await?;
    for x in values {
        let manifest: Uuid = x.get("id");
        let person: Uuid = x.get("person_id");
        let outcome = if x.get::<String, _>("disposition") != "eligible" {
            "held"
        } else {
            let target: Uuid = x
                .get::<Option<Uuid>, _>("target_id")
                .ok_or(MigrationError::Conflict)?;
            let sk: Vec<u8> = x.get("source_key");
            let claim:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM migration_metadata_catalog_claim WHERE organization_id=$1 AND source_account_id=$2 AND kind='field' AND source_key=$3 AND target_id=$4)").bind(org.0).bind(account).bind(&sk).bind(target).fetch_one(&mut *tx).await?;
            if !claim {
                "held"
            } else {
                let raw = crypto::open_snapshot(
                    key,
                    org,
                    snapshot,
                    x.get::<Uuid, _>("op_id"),
                    &format!("admitted-metadata-v1:{plan}:operation"),
                    &x.get::<Vec<u8>, _>("nonce"),
                    &x.get::<Vec<u8>, _>("ciphertext"),
                )
                .map_err(|_| MigrationError::Crypto)?;
                let v: serde_json::Value =
                    serde_json::from_slice(&raw).map_err(|_| MigrationError::Crypto)?;
                match v["value"]["value"].as_str() {
                    Some(text) => {
                        permit(&mut tx, root, plan, lease, manifest, target, "field", &sk).await?;
                        let old:Option<String>=sqlx::query_scalar("SELECT text_value FROM person_custom_field_value WHERE organization_id=$1 AND person_id=$2 AND field_id=$3").bind(org.0).bind(person).bind(target).fetch_optional(&mut *tx).await?.flatten();
                        if old.is_some() {
                            if old == Some(text.to_owned()) {
                                "already_present"
                            } else {
                                "held"
                            }
                        } else {
                            let n=sqlx::query("INSERT INTO person_custom_field_value(organization_id,person_id,field_id,field_type,text_value,updated_by_user_id,origin,correlation_id) SELECT $1,$2,$3,field_type,$4,$5,'migration',$6 FROM custom_field WHERE id=$3 AND organization_id=$1 AND field_type='text' AND archived_at IS NULL").bind(org.0).bind(person).bind(target).bind(text).bind(actor).bind(root).execute(&mut *tx).await?;
                            if n.rows_affected() == 1 {
                                "applied"
                            } else {
                                "held"
                            }
                        }
                    }
                    None => "held",
                }
            }
        };
        sqlx::query("UPDATE migration_admitted_metadata_operation SET disposition=$2 WHERE id=$1 AND organization_id=$3").bind(x.get::<Uuid,_>("op_id")).bind(outcome).bind(org.0).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE migration_admitted_metadata_import SET state='completed',phase='complete',lease_token=NULL,lease_expires_at=NULL WHERE id=$1 AND organization_id=$2 AND lease_token=$3").bind(root).bind(org.0).bind(lease).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}
