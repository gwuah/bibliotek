pub mod annotations;
pub mod documents;
pub mod matches;
pub mod uploads;

use libsql::Connection;
use libsql::params::IntoParams;

use crate::error::Result;

pub(crate) async fn returning_id(conn: &Connection, sql: &str, params: impl IntoParams) -> Result<i64> {
    let mut rows = conn.query(sql, params).await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| anyhow::anyhow!("statement returned no id: {sql}"))?;
    Ok(row.get(0)?)
}

pub(crate) fn id_array(ids: &[i64]) -> String {
    serde_json::Value::from(ids).to_string()
}
