// Copyright 2026 RisingWave Labs
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Simulation test for "dirty-table-only" commit_epoch (sparse tables_to_commit).
//!
//! Enable with:
//!   RW_ENABLE_SPARSE_TABLES_TO_COMMIT=1 cargo test -p risingwave_simulation ...
//!
//! This test is a starting point for scale experiments: it creates many MVs but writes
//! only a small amount of data, so the number of tables included in commit_epoch should
//! be close to the dirty set (and not scale with MV count).

use anyhow::Result;
use risingwave_simulation::cluster::{Cluster, Configuration};

#[tokio::test]
async fn test_sparse_commit_dirty_tables_smoke() -> Result<()> {
    let configuration = Configuration::for_scale();
    let mut cluster = Cluster::start(configuration).await?;

    cluster.run("CREATE TABLE events (user_id INT, attr INT);").await?;

    // Create a bunch of segment MVs.
    let mv_count = 50;
    for i in 0..mv_count {
        cluster
            .run(format!(
                "CREATE MATERIALIZED VIEW seg_{i} AS SELECT user_id FROM events WHERE attr = {i};"
            ))
            .await?;
    }

    // Only write to a tiny subset.
    cluster
        .run("INSERT INTO events VALUES (1, 1), (2, 1), (3, 1), (4, 2);")
        .await?;
    cluster.run("FLUSH").await?;

    Ok(())
}
