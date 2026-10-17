//! Geographic-correctness evaluation of the service and its index.
//!
//! ```text
//! evaluate run --suite smoke|gold|addresses --out DIR [--compare SNAPSHOT] [options]
//! evaluate run --corpus FILE --out DIR       JSONL or TSV corpus (unscored cases: drift only)
//! evaluate diff BASELINE CANDIDATE           run directories or .tsv/.json snapshots
//! evaluate snapshot RUN_DIR FILE.tsv         one row per case, plus FILE.manifest.json
//! evaluate snapshot RUN_DIR FILE.json        manifest and full per-case results in one file
//! evaluate report RUN_DIR [--rescore]        re-summarize a saved run, no network
//! evaluate build-gold                        regenerate tests/gold.jsonl from tests/gold.seeds.jsonl
//! evaluate sample-us --nad FILE --seed TEXT --out FILE   draw US cases from a NAD release, registry-first
//! evaluate sample-fr --ban FILE --seed TEXT --out FILE   draw French cases from a BAN CSV export
//! evaluate gold-corpus --out FILE            the gold suite as one file, for images
//! evaluate check                             validate the corpora and snapshot, no network
//! evaluate coverage --index DIR [--population countryInfo.txt]   house numbers per country
//! ```
//!
//! `run` starts the local service binary on the index in `--index` (or INDEX_DIR), unless `--rpc`
//! points at a running service. Direct checks load the same index into the evaluator; they need
//! it too.

mod ban;
mod coverage;
mod gold;
mod model;
mod nad;
mod report;
mod run;
mod score;

use anyhow::{Context, Result, bail};
use model::{Case, Record, load_corpus, load_jsonl};
use std::{collections::HashMap, path::PathBuf};

const USAGE: &str = "usage: evaluate <run|diff|snapshot|report|build-gold|sample-us|sample-fr|gold-corpus|check|coverage> [options]

run         --out DIR [--suite smoke|gold|addresses] [--corpus FILE] [--compare SNAPSHOT_OR_RUN]
            [--fail-on-drift] [--index DIR] [--rpc URL] [--only REGEX] [--max-rps 20] [--budget 20000] (limits default to none with the local service)
            [--no-direct] [--resume] [--backend-label TEXT] [--repeat 3]
diff        BASELINE CANDIDATE [--corpus FILE] [--out FILE] [--fail-on-drift] [--relative]
snapshot    RUN_DIR FILE.tsv|FILE.json
report      RUN_DIR [--corpus FILE] [--rescore]
build-gold  [--seeds FILE] [--out FILE] [--nad FILE] [--ban FILE]
sample-us   --nad FILE --seed TEXT --out FILE
sample-fr   --ban FILE --seed TEXT --out FILE
gold-corpus --out FILE      write the gold suite as one file (the curated corpus, then tests/registry/*.jsonl)
check       validate tests/gold.jsonl, tests/registry/*.jsonl, tests/addresses.tsv and its snapshot
coverage    --index DIR [--population countryInfo.txt] [--out FILE]   houses and streets per country, largest gap first";

struct Args {
    positional: Vec<String>,
    values: HashMap<String, String>,
    flags: Vec<String>,
}

impl Args {
    fn parse(raw: impl Iterator<Item = String>, valued: &[&str]) -> Result<Self> {
        let mut args = Args {
            positional: vec![],
            values: HashMap::new(),
            flags: vec![],
        };
        let mut raw = raw.peekable();
        while let Some(arg) = raw.next() {
            if let Some(name) = arg.strip_prefix("--") {
                if valued.contains(&name) {
                    let value = raw
                        .next()
                        .with_context(|| format!("--{name} needs a value"))?;
                    args.values.insert(name.into(), value);
                } else {
                    args.flags.push(name.into());
                }
            } else {
                args.positional.push(arg);
            }
        }
        Ok(args)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    fn path(&self, name: &str) -> Option<PathBuf> {
        self.value(name).map(PathBuf::from)
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }

    fn only_flags(&self, allowed: &[&str]) -> Result<()> {
        if let Some(unknown) = self.flags.iter().find(|f| !allowed.contains(&f.as_str())) {
            bail!("unknown option --{unknown}\n\n{USAGE}");
        }
        Ok(())
    }
}

/// `--corpus FILE`, else the gold suite (the curated corpus and the registry-first ones).
fn corpus(args: &Args) -> Result<Vec<Case>> {
    match args.path("corpus") {
        Some(path) => load_corpus(&path),
        None => model::load_suite(),
    }
}

#[tokio::main]
async fn main() {
    match command().await {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(2);
        }
    }
}

async fn command() -> Result<i32> {
    let mut raw = std::env::args().skip(1);
    let command = raw.next().unwrap_or_default();
    let valued = [
        "suite",
        "corpus",
        "out",
        "index",
        "rpc",
        "only",
        "max-rps",
        "budget",
        "backend-label",
        "compare",
        "repeat",
        "seeds",
        "nad",
        "ban",
        "seed",
        "population",
    ];
    let args = Args::parse(raw, &valued)?;
    match command.as_str() {
        "run" => {
            args.only_flags(&["fail-on-drift", "no-direct", "resume"])?;
            let suite = args.value("suite").unwrap_or("gold").to_string();
            if !["smoke", "gold", "addresses"].contains(&suite.as_str()) {
                bail!("--suite must be smoke, gold or addresses");
            }
            let rpc = args.value("rpc").map(String::from);
            if rpc.is_some() && args.value("repeat").is_some() {
                bail!(
                    "--repeat times the local service only; it would bypass --max-rps with --rpc"
                );
            }
            // Limits protect an external backend; the spawned local service needs none.
            let local = rpc.is_none();
            let options = run::Options {
                suite,
                corpus: args.path("corpus"),
                out: args.path("out").context("--out is required")?,
                index: args.path("index"),
                repeat: match &rpc {
                    Some(_) => 1,
                    None => args.value("repeat").map_or(Ok(3), str::parse)?,
                }
                .max(1),
                rpc,
                only: args.value("only").map(regex::Regex::new).transpose()?,
                max_rps: args
                    .value("max-rps")
                    .map_or(Ok(if local { 1e6 } else { 20. }), str::parse)?,
                budget: args
                    .value("budget")
                    .map_or(Ok(if local { usize::MAX } else { 20_000 }), str::parse)?,
                direct: !args.flag("no-direct"),
                resume: args.flag("resume"),
                backend_label: args.value("backend-label").unwrap_or_default().into(),
                compare: args.path("compare"),
                fail_on_drift: args.flag("fail-on-drift"),
            };
            run::run(options).await
        }
        "diff" => {
            args.only_flags(&["fail-on-drift", "relative"])?;
            let [baseline, candidate] = args.positional.as_slice() else {
                bail!("diff needs BASELINE and CANDIDATE\n\n{USAGE}");
            };
            let failed = report::diff(
                &corpus(&args)?,
                baseline.as_ref(),
                candidate.as_ref(),
                args.path("out").as_deref(),
                args.flag("fail-on-drift"),
                args.flag("relative"),
            )?;
            Ok(i32::from(failed))
        }
        "snapshot" => {
            args.only_flags(&[])?;
            let [run_dir, out] = args.positional.as_slice() else {
                bail!("snapshot needs RUN_DIR and FILE.tsv or FILE.json\n\n{USAGE}");
            };
            report::snapshot(run_dir.as_ref(), out.as_ref())?;
            Ok(0)
        }
        "report" => {
            args.only_flags(&["rescore"])?;
            let [run_dir] = args.positional.as_slice() else {
                bail!("report needs RUN_DIR\n\n{USAGE}");
            };
            let run_dir = PathBuf::from(run_dir);
            let cases = corpus(&args)?;
            let manifest =
                serde_json::from_str(&std::fs::read_to_string(run_dir.join("manifest.json"))?)?;
            let mut results = load_jsonl::<Record>(&run_dir.join("results.jsonl"))?;
            if args.flag("rescore") {
                rescore(&cases, &mut results);
            }
            let (text, failed) = report::summarize(&cases, &results, &manifest);
            std::fs::write(run_dir.join("report.md"), &text)?;
            print!("{text}");
            Ok(i32::from(failed))
        }
        "build-gold" => {
            args.only_flags(&[])?;
            let seeds = args
                .path("seeds")
                .unwrap_or_else(|| model::root().join("tests/gold.seeds.jsonl"));
            gold::build(
                &seeds,
                &args.path("out").unwrap_or_else(model::gold),
                args.path("nad").as_deref(),
                args.path("ban").as_deref(),
            )
            .await?;
            Ok(0)
        }
        "gold-corpus" => {
            args.only_flags(&[])?;
            model::write_suite(&args.path("out").context("--out is required")?)?;
            Ok(0)
        }
        "sample-us" | "sample-fr" => {
            args.only_flags(&[])?;
            let seed = args.value("seed").context("--seed is required")?;
            let lines = if command == "sample-fr" {
                ban::sample(&args.path("ban").context("--ban is required")?, seed)?
            } else {
                nad::sample(&args.path("nad").context("--nad is required")?, seed)?
            };
            let out = args.path("out").context("--out is required")?;
            let text: Vec<_> = lines
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<_, _>>()?;
            std::fs::write(&out, text.join("\n") + "\n")?;
            eprintln!("{} seed lines -> {}", lines.len(), out.display());
            Ok(0)
        }
        "check" => {
            args.only_flags(&[])?;
            let cases = load_corpus(&model::gold())?;
            model::validate_gold(&cases)?;
            let mut registry = 0;
            for path in model::registry_corpora() {
                let cases = load_corpus(&path)?;
                model::validate_gold(&cases).with_context(|| path.display().to_string())?;
                registry += cases.len();
            }
            // the suite runs them together: ids and queries must not repeat across files
            model::validate_gold(&model::load_suite()?)?;
            let addresses =
                model::validate_addresses(&model::addresses(), &model::address_snapshot())?;
            println!(
                "{} gold cases, {registry} registry cases and {addresses} address cases validated",
                cases.len()
            );
            Ok(0)
        }
        "coverage" => {
            args.only_flags(&[])?;
            let index = args
                .path("index")
                .or_else(|| std::env::var_os("INDEX_DIR").map(PathBuf::from))
                .context("--index or INDEX_DIR is required")?;
            let population = match args.path("population") {
                Some(path) => coverage::populations(&path)?,
                None => HashMap::new(),
            };
            let table = coverage::table(&coverage::count(&index)?, &population);
            match args.path("out") {
                Some(out) => std::fs::write(out, table)?,
                None => print!("{table}"),
            }
            Ok(0)
        }
        _ => {
            eprintln!("{USAGE}");
            Ok(2)
        }
    }
}

/// Re-scores saved responses against the current corpus, e.g. after an expectation change,
/// and lists every case whose class changed. No network; direct-backend evidence is kept as recorded.
fn rescore(cases: &[Case], results: &mut [Record]) {
    let by_id: HashMap<_, _> = cases.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut changed = 0;
    for record in results.iter_mut() {
        let Some(case) = by_id.get(record.id.as_str()) else {
            continue;
        };
        let suggest_before = record.suggest.clone();
        run::rescore_suggest(case, record);
        if record.suggest != suggest_before {
            changed += 1;
            eprintln!("rescored {}: suggestions re-ranked", record.id);
        }
        let locate_before = (record.reverse.clone(), record.region.clone());
        run::rescore_locate(case, record);
        if (record.reverse.clone(), record.region.clone()) != locate_before {
            changed += 1;
            eprintln!(
                "rescored {}: reverse or region answers re-scored",
                record.id
            );
        }
        let before = record.class.clone();
        let response = record.response.clone();
        let error = record.error.clone();
        match (&response, &error) {
            (Some(response), _) => run::classify(case, Ok(response), record),
            (None, Some(error)) => run::classify(case, Err(error), record),
            (None, None) => continue,
        }
        if record.class != before {
            changed += 1;
            eprintln!("rescored {}: {before} -> {}", record.id, record.class);
        }
    }
    eprintln!("rescore: {changed} of {} classes changed", results.len());
}
