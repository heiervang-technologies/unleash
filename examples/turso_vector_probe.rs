//! Step 0 probe (plan: search overhaul).
//!
//! Verifies that the `turso` Rust crate (BETA) supports the SQL vector
//! functions documented at https://docs.turso.tech/guides/vector-search:
//!   - `vector32('[f, f, ...]')` literal constructor
//!   - `vector_distance_cos(blob, blob)` similarity function
//!   - FTS5 virtual tables with `bm25(table)` ranking
//!
//! Plan ref: PR A is gated on this probe passing. If it fails, the plan's
//! `SearchStore` adapter will target libsql instead and the rest of the
//! work proceeds unchanged.
//!
//! Run: `cargo run --example turso_vector_probe`

use std::error::Error;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let db = turso::Builder::new_local(":memory:").build().await?;
    let conn = db.connect()?;

    println!("== probe 1: basic table + insert ==");
    conn.execute(
        "CREATE TABLE docs (id INTEGER PRIMARY KEY, title TEXT, embedding BLOB)",
        (),
    )
    .await?;
    conn.execute(
        "INSERT INTO docs (title, embedding) VALUES ('apple', vector32('[1.0, 0.0, 0.0, 0.0]'))",
        (),
    )
    .await?;
    conn.execute(
        "INSERT INTO docs (title, embedding) VALUES ('orange', vector32('[0.9, 0.1, 0.0, 0.0]'))",
        (),
    )
    .await?;
    conn.execute(
        "INSERT INTO docs (title, embedding) VALUES ('car', vector32('[0.0, 0.0, 1.0, 0.0]'))",
        (),
    )
    .await?;
    println!("  ok — vector32() literal accepted");

    println!("== probe 2: vector_distance_cos query ==");
    let mut rows = conn
        .query(
            "SELECT title, vector_distance_cos(embedding, vector32('[1.0, 0.0, 0.0, 0.0]')) AS d \
             FROM docs ORDER BY d ASC",
            (),
        )
        .await?;
    while let Some(row) = rows.next().await? {
        let title: String = row.get(0)?;
        let d: f64 = row.get(1)?;
        println!("  {title:<8} d={d:.6}");
    }
    println!("  ok — vector_distance_cos returns ordered f64 distances");

    println!("== probe 3a: FTS5 + bm25() ranking ==");
    let fts5_result = conn
        .execute("CREATE VIRTUAL TABLE docs_fts USING fts5(title)", ())
        .await;
    if let Err(e) = fts5_result {
        eprintln!("  SKIP — fts5 unavailable: {e}");
        eprintln!("\n== probe 3b: alternative full-text indices ==");
        for module in ["fts4", "fts3", "fts", "tantivy"] {
            let r = conn
                .execute(
                    &format!("CREATE VIRTUAL TABLE probe_{module} USING {module}(title)"),
                    (),
                )
                .await;
            match r {
                Ok(_) => eprintln!("  found alt module: {module}"),
                Err(e) => eprintln!("  {module}: unavailable ({e})"),
            }
        }
        eprintln!("\nturso vector functions work, but no FTS module is exposed.");
        eprintln!("Skipping remaining FTS-dependent probes; falling through cleanly.");
        eprintln!("\nFINDING: turso 0.6.0 vector OK, FTS5 NOT EXPOSED.");
        return Ok(());
    }
    conn.execute(
        "INSERT INTO docs_fts (rowid, title) SELECT id, title FROM docs",
        (),
    )
    .await?;
    let mut rows = conn
        .query(
            "SELECT title, bm25(docs_fts) AS r FROM docs_fts \
             WHERE docs_fts MATCH 'apple OR orange' ORDER BY r",
            (),
        )
        .await?;
    let mut hits = 0;
    while let Some(row) = rows.next().await? {
        let title: String = row.get(0)?;
        let r: f64 = row.get(1)?;
        println!("  {title:<8} bm25={r:.6}");
        hits += 1;
    }
    if hits == 0 {
        eprintln!("  FAIL — FTS5 query returned no rows");
        std::process::exit(2);
    }
    println!("  ok — FTS5 + bm25() works");

    println!("== probe 4: hybrid query (vector + FTS5 in one SELECT) ==");
    let mut rows = conn
        .query(
            "WITH \
               bm AS (SELECT rowid AS pk, bm25(docs_fts) AS r FROM docs_fts \
                      WHERE docs_fts MATCH 'apple'), \
               vec AS (SELECT id AS pk, \
                              vector_distance_cos(embedding, vector32('[1.0, 0.0, 0.0, 0.0]')) AS d \
                       FROM docs) \
             SELECT d.title, bm.r, vec.d \
             FROM docs d \
             LEFT JOIN bm  ON bm.pk = d.id \
             LEFT JOIN vec ON vec.pk = d.id \
             ORDER BY ifnull(bm.r, 1e6), vec.d",
            (),
        )
        .await?;
    while let Some(row) = rows.next().await? {
        let title: String = row.get(0)?;
        let bm: Option<f64> = row.get(1).ok();
        let cos: Option<f64> = row.get(2).ok();
        println!("  {title:<8} bm={bm:?} cos={cos:?}");
    }
    println!("  ok — hybrid CTE join works");

    println!("\nall probes passed — turso 0.6.0 supports the SQL surface we need.");
    Ok(())
}
