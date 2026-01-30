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

//! Baseline simulation scenario for CDP-like segment membership with tiered freshness.
//!
//! Goal: provide a harness to create many MVs across multiple databases, each database configured
//! with a different barrier interval/checkpoint frequency (e.g., 5s / 5min / 1h).
//!
//! This test is intentionally a "smoke" baseline. Scaling knobs (MV counts, write patterns) can be
//! increased in follow-up tests to stress barrier coordination + Hummock commit behavior.

use anyhow::Result;
use risingwave_simulation::cluster::{Cluster, Configuration};

#[tokio::test]
async fn test_cdp_segment_tiers_smoke() -> Result<()> {
    let configuration = Configuration::for_scale();
    let mut cluster = Cluster::start(configuration).await?;

    // Create tiered databases.
    cluster.run("CREATE DATABASE hot_tier").await?;
    cluster.run("CREATE DATABASE warm_tier").await?;
    cluster.run("CREATE DATABASE cold_tier").await?;

    // Configure per-database barrier interval / checkpoint frequency.
    // NOTE: these settings currently require the ResourceGroup feature in non-test builds.
    cluster
        .run("ALTER DATABASE hot_tier SET BARRIER_INTERVAL_MS = 5000")
        .await?;
    cluster
        .run("ALTER DATABASE warm_tier SET BARRIER_INTERVAL_MS = 300000")
        .await?;
    cluster
        .run("ALTER DATABASE cold_tier SET BARRIER_INTERVAL_MS = 3600000")
        .await?;

    cluster
        .run("ALTER DATABASE hot_tier SET CHECKPOINT_FREQUENCY = 1")
        .await?;
    cluster
        .run("ALTER DATABASE warm_tier SET CHECKPOINT_FREQUENCY = 10")
        .await?;
    cluster
        .run("ALTER DATABASE cold_tier SET CHECKPOINT_FREQUENCY = 1")
        .await?;

    // Create an events table and a few segment membership MVs per tier.
    for db in ["hot_tier", "warm_tier", "cold_tier"] {
        let mut session = cluster.start_session_in_db(db);
        session
            .run(
                "CREATE TABLE events (user_id INT, attr INT, ts TIMESTAMPTZ DEFAULT NOW());",
            )
            .await?;
        session
            .run("CREATE MATERIALIZED VIEW seg_attr_1 AS SELECT user_id FROM events WHERE attr = 1;")
            .await?;
        session
            .run("CREATE MATERIALIZED VIEW seg_attr_2 AS SELECT user_id FROM events WHERE attr = 2;")
            .await?;

        // Insert a small batch and flush to drive barrier/commit.
        session
            .run("INSERT INTO events(user_id, attr) VALUES (1, 1), (2, 2), (3, 1);")
            .await?;
        session.run("FLUSH").await?;
    }

    Ok(())
}
