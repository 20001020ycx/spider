//! End-to-end test: resource-group isolation across worker pools.
//!
//! Verifies the two guarantees a dedicated worker pool must uphold when the deployment pins a
//! worker to a resource group (as the Helm chart's `spiderConfig.worker.pools[].resource_group`
//! does):
//!
//! 1. A dedicated worker executes only its configured resource group's tasks.
//! 2. A dedicated worker never executes another resource group's tasks, even while those tasks are
//!    actively being scheduled onto the general workers.
//!
//! The deployment under test must provide a general pool plus a pool dedicated to [`DEDICATED_RG`],
//! both loading the `integration_test_tasks` package, and must forward
//! `SPIDER_EXTERNAL_RESOURCE_GROUP_ID` to executors via the execution manager's `inherited_env`.
//! Each job runs a single [`report_worker_pool`] task whose output names the pool that ran it
//! (the dedicated worker reports [`DEDICATED_RG`]; a general worker reports `"general"`), so the
//! placement of every task is observed directly from job outputs.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use e2e::JobSubmission;
use e2e::SpiderTestDriver;
use e2e::TerminationResult;
use e2e::decode_output;
use e2e::encode_input;
use spider_core::task::DataTypeDescriptor;
use spider_core::task::TaskDescriptor;
use spider_core::task::TaskGraph;
use spider_core::task::TdlContext;
use spider_core::task::ValueTypeDescriptor;
use tokio::task::JoinSet;

/// External id of the resource group a dedicated worker pool is pinned to. Must match the
/// deployment's dedicated pool `resource_group.external_id` and the identity that pool's worker
/// reports through `SPIDER_EXTERNAL_RESOURCE_GROUP_ID`.
const DEDICATED_RG: &str = "rg-dedicated";

/// External id of a resource group with no dedicated pool; its tasks run on the general workers.
const OTHER_RG: &str = "rg-other";

/// Identity a general (unpinned) worker reports.
const GENERAL_POOL: &str = "general";

/// TDL package and task the deployment's workers load to report their pool.
const TASK_PACKAGE: &str = "integration_test_tasks";
const TASK_FUNC: &str = "report_worker_pool";

/// Number of jobs submitted per resource group.
const JOBS_PER_GROUP: usize = 60;

/// Per-task sleep, widening the window in which tasks from both groups are concurrently in flight
/// so the dedicated worker has ample opportunity to (wrongly) pick up an [`OTHER_RG`] task, and so
/// the workload outlasts the dedicated worker's startup.
const TASK_SLEEP_MILLIS: i64 = 500;

/// Upper bound on how long a single job may take to reach a terminal state.
const JOB_TIMEOUT: Duration = Duration::from_secs(180);

#[tokio::test]
async fn test_resource_group_isolation() -> anyhow::Result<()> {
    // (submitted resource group, pool that executed the task).
    let placements: Arc<Mutex<Vec<(&'static str, String)>>> = Arc::new(Mutex::new(Vec::new()));

    let mut jobs = JoinSet::new();
    for _ in 0..JOBS_PER_GROUP {
        for resource_group in [DEDICATED_RG, OTHER_RG] {
            let placements = Arc::clone(&placements);
            jobs.spawn(async move {
                run_report_job(resource_group, &placements).await
            });
        }
    }
    while let Some(result) = jobs.join_next().await {
        result.context("report-job task panicked")??;
    }

    let placements = placements.lock().expect("placements mutex poisoned");

    // Diagnostic: (submitted resource group, executing pool) -> count.
    let mut counts: std::collections::BTreeMap<(&str, String), usize> =
        std::collections::BTreeMap::new();
    for (submitted_rg, pool) in placements.iter() {
        *counts.entry((submitted_rg, pool.clone())).or_default() += 1;
    }
    eprintln!("resource-group isolation placement (submitted_rg, executing_pool) -> count:");
    for ((submitted_rg, pool), count) in &counts {
        eprintln!("  ({submitted_rg}, {pool}) -> {count}");
    }

    // Property 2: the dedicated worker never ran a task from another resource group, even though
    // OTHER_RG's tasks were being actively scheduled onto the general workers throughout.
    let poached: Vec<&(&str, String)> = placements
        .iter()
        .filter(|(submitted_rg, pool)| pool == DEDICATED_RG && *submitted_rg != DEDICATED_RG)
        .collect();
    anyhow::ensure!(
        poached.is_empty(),
        "dedicated worker executed {} task(s) from another resource group: {poached:?}",
        poached.len(),
    );

    // Property 1 (non-vacuous): the dedicated worker did run its own group's tasks — otherwise the
    // "no poaching" check above could pass simply because the dedicated worker was never used.
    let dedicated_ran_own = placements
        .iter()
        .filter(|(submitted_rg, pool)| pool == DEDICATED_RG && *submitted_rg == DEDICATED_RG)
        .count();
    anyhow::ensure!(
        dedicated_ran_own > 0,
        "dedicated worker executed none of its own resource group's tasks; the dedicated pool may \
         not have registered (check that {DEDICATED_RG} exists and its credential matches)",
    );

    // Sanity: OTHER_RG's tasks all completed (so they were genuinely scheduled and run — on the
    // general workers, since no dedicated pool serves OTHER_RG).
    let other_completed = placements
        .iter()
        .filter(|(submitted_rg, _)| *submitted_rg == OTHER_RG)
        .count();
    anyhow::ensure!(
        other_completed == JOBS_PER_GROUP,
        "expected {JOBS_PER_GROUP} {OTHER_RG} jobs to complete, got {other_completed}",
    );
    for (_, pool) in placements.iter().filter(|(rg, _)| *rg == OTHER_RG) {
        anyhow::ensure!(
            pool == GENERAL_POOL,
            "an {OTHER_RG} task ran on pool {pool:?}, expected {GENERAL_POOL:?}",
        );
    }

    Ok(())
}

/// Submits one single-task job under `resource_group` and records the pool that executed it.
///
/// # Errors
///
/// Returns an error if the job does not succeed or its output cannot be decoded.
async fn run_report_job(
    resource_group: &'static str,
    placements: &Arc<Mutex<Vec<(&'static str, String)>>>,
) -> anyhow::Result<()> {
    let submission = single_report_job(resource_group)?;
    let placements = Arc::clone(placements);
    SpiderTestDriver::run(submission, JOB_TIMEOUT, async move |_job_id, result| {
        let outputs = match result {
            TerminationResult::Success(outputs) => outputs,
            TerminationResult::Failure(message) => bail!("job failed: {message}"),
            TerminationResult::Cancelled => bail!("job cancelled"),
        };
        anyhow::ensure!(
            outputs.len() == 1,
            "expected exactly one output, got {}",
            outputs.len(),
        );
        let pool = String::from_utf8(decode_output::<Vec<u8>>(&outputs[0])?)
            .context("worker pool identity is not valid UTF-8")?;
        placements
            .lock()
            .expect("placements mutex poisoned")
            .push((resource_group, pool));
        Ok(())
    })
    .await
}

/// Builds a job that runs a single [`report_worker_pool`] task under `resource_group`.
///
/// # Errors
///
/// Forwards [`TaskGraph::new`] / [`TaskGraph::insert_task`] / [`encode_input`] failures.
fn single_report_job(resource_group: &str) -> anyhow::Result<JobSubmission> {
    let int64 = DataTypeDescriptor::Value(ValueTypeDescriptor::int64());
    let bytes = DataTypeDescriptor::Value(ValueTypeDescriptor::bytes());
    let mut task_graph = TaskGraph::new(None, None)?;
    task_graph.insert_task(TaskDescriptor {
        tdl_context: TdlContext {
            package: TASK_PACKAGE.to_owned(),
            task_func: TASK_FUNC.to_owned(),
        },
        execution_policy: None,
        inputs: vec![int64],
        outputs: vec![bytes],
        input_sources: None,
    })?;
    Ok(JobSubmission {
        resource_group_id: resource_group.to_owned(),
        task_graph,
        inputs: vec![encode_input(&TASK_SLEEP_MILLIS)?],
    })
}
