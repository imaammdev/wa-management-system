//! Workspace configuration: org profile, labels, canned responses, pipelines.

use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{Error, Result};

#[derive(Debug, Serialize, FromRow)]
pub struct OrgSettings {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub timezone: String,
    pub locale: String,
    pub settings: Value,
}

pub async fn get_settings(pool: &PgPool, org_id: Uuid) -> Result<OrgSettings> {
    sqlx::query_as::<_, OrgSettings>(
        "SELECT id, name, slug, timezone, locale, settings FROM organizations WHERE id=$1",
    )
    .bind(org_id)
    .fetch_optional(pool)
    .await?
    .ok_or(Error::NotFound("organization"))
}

/// Merges the supplied keys into `settings` rather than replacing the object,
/// so a partial save from one settings tab can't wipe another's values.
pub async fn update_settings(
    pool: &PgPool,
    org_id: Uuid,
    name: Option<&str>,
    timezone: Option<&str>,
    patch: Option<&Value>,
) -> Result<OrgSettings> {
    if let Some(n) = name {
        if n.trim().is_empty() {
            return Err(Error::validation("nama organisasi tidak boleh kosong"));
        }
    }
    sqlx::query(
        r#"UPDATE organizations SET
             name     = COALESCE($2, name),
             timezone = COALESCE($3, timezone),
             settings = settings || COALESCE($4, '{}'::jsonb),
             updated_at = now()
           WHERE id = $1"#,
    )
    .bind(org_id)
    .bind(name)
    .bind(timezone)
    .bind(patch)
    .execute(pool)
    .await?;
    get_settings(pool, org_id).await
}

// ------------------------------------------------------------------- labels

#[derive(Debug, Serialize, FromRow)]
pub struct LabelRow {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    pub usage: i64,
}

pub async fn labels(pool: &PgPool, org_id: Uuid) -> Result<Vec<LabelRow>> {
    Ok(sqlx::query_as::<_, LabelRow>(
        r#"SELECT l.id, l.name, l.color,
                  (SELECT count(*) FROM conversation_labels cl WHERE cl.label_id = l.id)
                + (SELECT count(*) FROM contact_labels ct WHERE ct.label_id = l.id) AS usage
           FROM labels l WHERE l.org_id=$1 ORDER BY l.name"#,
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?)
}

pub async fn update_label(
    pool: &PgPool,
    org_id: Uuid,
    id: Uuid,
    name: Option<&str>,
    color: Option<&str>,
) -> Result<()> {
    let r = sqlx::query(
        "UPDATE labels SET name=COALESCE($3,name), color=COALESCE($4,color)
         WHERE id=$1 AND org_id=$2",
    )
    .bind(id)
    .bind(org_id)
    .bind(name)
    .bind(color)
    .execute(pool)
    .await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound("label"));
    }
    Ok(())
}

pub async fn delete_label(pool: &PgPool, org_id: Uuid, id: Uuid) -> Result<()> {
    let r = sqlx::query("DELETE FROM labels WHERE id=$1 AND org_id=$2")
        .bind(id)
        .bind(org_id)
        .execute(pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound("label"));
    }
    Ok(())
}

// --------------------------------------------------------- canned responses

pub async fn delete_canned(pool: &PgPool, org_id: Uuid, id: Uuid) -> Result<()> {
    let r = sqlx::query("DELETE FROM canned_responses WHERE id=$1 AND org_id=$2")
        .bind(id)
        .bind(org_id)
        .execute(pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(Error::NotFound("canned response"));
    }
    Ok(())
}

// ---------------------------------------------------------------- pipelines

#[derive(Debug, Serialize, FromRow)]
pub struct StageRow {
    pub id: Uuid,
    pub name: String,
    pub position: i32,
    pub deals: i64,
}

#[derive(Debug, Serialize)]
pub struct PipelineRow {
    pub id: Uuid,
    pub name: String,
    pub is_default: bool,
    pub stages: Vec<StageRow>,
}

pub async fn pipelines(pool: &PgPool, org_id: Uuid) -> Result<Vec<PipelineRow>> {
    let heads: Vec<(Uuid, String, bool)> = sqlx::query_as(
        "SELECT id, name, is_default FROM pipelines WHERE org_id=$1 ORDER BY is_default DESC, name",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(heads.len());
    for (id, name, is_default) in heads {
        let stages = sqlx::query_as::<_, StageRow>(
            r#"SELECT s.id, s.name, s.position,
                      (SELECT count(*) FROM deals d WHERE d.stage_id = s.id) AS deals
               FROM pipeline_stages s WHERE s.pipeline_id=$1 ORDER BY s.position"#,
        )
        .bind(id)
        .fetch_all(pool)
        .await?;
        out.push(PipelineRow {
            id,
            name,
            is_default,
            stages,
        });
    }
    Ok(out)
}

pub async fn add_stage(pool: &PgPool, org_id: Uuid, pipeline_id: Uuid, name: &str) -> Result<()> {
    let owns: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pipelines WHERE id=$1 AND org_id=$2)")
            .bind(pipeline_id)
            .bind(org_id)
            .fetch_one(pool)
            .await?;
    if !owns {
        return Err(Error::NotFound("pipeline"));
    }
    sqlx::query(
        r#"INSERT INTO pipeline_stages (pipeline_id, name, position)
           VALUES ($1, $2,
                   COALESCE((SELECT max(position) + 1 FROM pipeline_stages WHERE pipeline_id=$1), 0))
           ON CONFLICT (pipeline_id, name) DO NOTHING"#,
    )
    .bind(pipeline_id)
    .bind(name.trim())
    .execute(pool)
    .await?;
    Ok(())
}
