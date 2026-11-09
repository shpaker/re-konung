//! The headless `simulate` report over the shipped files of both editions
//! (skipped when `original/` is absent — docs/k1/editions.md).
//!
//! Issue #82: the runs over every map of both editions must exit clean,
//! the load-time refusals must be exactly the confirmed data facts of
//! docs/k1/creatures.md («Load-time refusals»), nothing may be trimmed,
//! and the goal-reach counters of the village runs must count real
//! arrivals — deterministically under the seed.

use std::path::PathBuf;

use kn_viewer::cli::{Cli, SourceArg};
use kn_viewer::{GoalReachCount, simulate_reports};

/// The run long enough for the slowest confirmed order to complete: the
/// map 1 static-work arrival lands around tick 3600 (the walk rolls of
/// the jobless crowd keep the merchant's neighbours busy).
const TICKS: u64 = 5000;
/// The seed of `simulate`'s default — the same run the CLI prints.
const SEED: u64 = 42;

fn game_dir(edition: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../original/k1")
        .join(edition)
        .join("game");
    dir.is_dir().then_some(dir)
}

fn editions() -> Vec<(&'static str, PathBuf)> {
    let found = ["rus", "eng"]
        .into_iter()
        .filter_map(|edition| game_dir(edition).map(|dir| (edition, dir)))
        .collect::<Vec<_>>();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../original/k1");
    assert!(
        !root.is_dir() || !found.is_empty(),
        "{} exists but holds no edition with a game/ folder: the tests on data would \
         not have run. How to unpack one — docs/k1/editions.md",
        root.display()
    );
    found
}

fn cli(game_dir: PathBuf) -> Cli {
    Cli {
        game_dir,
        map: None,
        source: SourceArg::Game,
        light_tick: None,
        screenshot: None,
        benchmark: None,
        command: None,
    }
}

/// `simulate --all` must accept every map of every edition: refusals and
/// trims are data the report presents (issue #82), so the run carries no
/// error verdict — and the refusals are exactly the confirmed facts, no
/// more and no fewer.
#[test]
fn every_map_of_every_edition_simulates_clean_and_refuses_only_the_documented_records() {
    // Map number → refused roster records, in report order. The facts and
    // their diagnosis: docs/k1/creatures.md, «Load-time refusals».
    const REFUSALS: &[(u32, &[(usize, &str)])] = &[
        (7, &[(433, "CellTaken")]),
        (10, &[(507, "ImpassableCell")]),
        (17, &[(513, "ImpassableCell")]),
        (25, &[(517, "ImpassableCell"), (518, "ImpassableCell")]),
        (30, &[(520, "ImpassableCell")]),
        (33, &[(838, "ImpassableCell")]),
        (37, &[(482, "ImpassableCell")]),
        (42, &[(525, "ImpassableCell")]),
        (44, &[(527, "ImpassableCell")]),
        (47, &[(529, "ImpassableCell")]),
    ];
    for (edition, game_dir) in editions() {
        let reports = simulate_reports(&cli(game_dir), None, true, TICKS, SEED)
            .unwrap_or_else(|error| panic!("{edition}: simulate --all failed: {error}"));
        assert!(
            reports.len() >= 40,
            "{edition}: only {} maps",
            reports.len()
        );
        let actual: Vec<(u32, Vec<(usize, String)>)> = reports
            .iter()
            .filter(|report| !report.unplaced.is_empty())
            .map(|report| {
                let records = report
                    .unplaced
                    .iter()
                    .map(|entry| match entry.split_once(": ") {
                        Some((label, reason)) => {
                            let record = label
                                .strip_prefix("roster ")
                                .unwrap_or_else(|| panic!("{edition}: odd entry {entry:?}"))
                                .parse::<usize>()
                                .unwrap_or_else(|_| panic!("{edition}: odd entry {entry:?}"));
                            (record, (*reason).to_owned())
                        }
                        None => panic!("{edition}: odd entry {entry:?}"),
                    })
                    .collect();
                (report.number, records)
            })
            .collect();
        let expected: Vec<(u32, Vec<(usize, String)>)> = REFUSALS
            .iter()
            .map(|(number, records)| {
                (
                    *number,
                    records
                        .iter()
                        .map(|(record, reason)| (*record, (*reason).to_owned()))
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            actual, expected,
            "{edition}: the load-time refusals moved off the documented facts"
        );
        for report in &reports {
            assert!(
                report.trimmed.is_empty(),
                "{edition}: map {} trimmed {:?} — the shipped areas hold room \
                 for every member, so a trim means the load rule regressed",
                report.number,
                report.trimmed,
            );
            assert!(
                report.errors.is_empty(),
                "{edition}: map {} failed orders: {:?}",
                report.number,
                report.errors,
            );
        }
    }
}

/// The village runs count real arrivals of the confirmed rules — the walk
/// rolls inside the spawn areas and the map 1 villager joining the occupied
/// merchant's place — and the same seed replays them identically.
#[test]
fn village_runs_count_arrivals_and_replay_under_the_same_seed() {
    for (edition, game_dir) in editions() {
        let runs =
            || simulate_reports(&cli(game_dir.clone()), None, true, TICKS, SEED).expect("simulate");
        let reports = runs();
        let by_number = |number: u32| {
            reports
                .iter()
                .find(|report| report.number == number)
                .unwrap_or_else(|| panic!("{edition}: map {number} is not in the report"))
        };

        // Walk arrivals of the jobless roll on the two reference villages.
        for number in [16, 19] {
            let report = by_number(number);
            let walk: usize = report.goal_reaches.iter().map(|reach| reach.walk).sum();
            assert!(
                walk > 0,
                "{edition}: map {number} counted no walk arrivals: {:?}",
                report.goal_reaches,
            );
            assert!(
                report
                    .goal_reaches
                    .iter()
                    .all(|reach| reach.workplace + reach.walk > 0),
                "{edition}: map {number} reports an empty counter: {:?}",
                report.goal_reaches,
            );
        }

        // The static-work arrival: record 369 of map 1 carries the merchant
        // job bits, its slot is occupied by 357, and the order sends it to a
        // free neighbour of the place (docs/k1/creatures.md, «Goal reach»).
        let map1 = by_number(1);
        let merchant = map1
            .goal_reaches
            .iter()
            .find(|reach| reach.id == 369)
            .unwrap_or_else(|| {
                panic!(
                    "{edition}: map 1 record 369 never arrived: {:?}",
                    map1.goal_reaches
                )
            });
        assert!(
            merchant.workplace >= 1,
            "{edition}: the merchant's neighbour of map 1 never reached the \
             place: {merchant:?}"
        );

        // Determinism: the same seed, the same counters and the same steps.
        let replay = runs();
        assert_eq!(
            replay, reports,
            "{edition}: two runs of the same seed differ"
        );
    }
}

/// The counters carry only creatures with at least one arrival, and the
/// totals agree with what the report lines sum to — a guard against a
/// counter that logs positions instead of counting arrivals.
#[test]
fn counters_are_per_creature_totals() {
    let Some((edition, game_dir)) = editions().into_iter().next() else {
        return;
    };
    let reports = simulate_reports(&cli(game_dir), Some(19), false, TICKS, SEED).expect("simulate");
    assert_eq!(reports.len(), 1);
    let report = &reports[0];
    let known: &[GoalReachCount] = &[
        GoalReachCount {
            id: 186,
            workplace: 0,
            walk: 15,
        },
        GoalReachCount {
            id: 187,
            workplace: 0,
            walk: 16,
        },
        GoalReachCount {
            id: 188,
            workplace: 0,
            walk: 12,
        },
        GoalReachCount {
            id: 189,
            workplace: 0,
            walk: 10,
        },
        GoalReachCount {
            id: 190,
            workplace: 0,
            walk: 16,
        },
        GoalReachCount {
            id: 191,
            workplace: 0,
            walk: 16,
        },
        GoalReachCount {
            id: 192,
            workplace: 0,
            walk: 15,
        },
        GoalReachCount {
            id: 193,
            workplace: 0,
            walk: 13,
        },
        GoalReachCount {
            id: 194,
            workplace: 0,
            walk: 14,
        },
        GoalReachCount {
            id: 195,
            workplace: 0,
            walk: 14,
        },
        GoalReachCount {
            id: 196,
            workplace: 0,
            walk: 13,
        },
        GoalReachCount {
            id: 197,
            workplace: 0,
            walk: 12,
        },
        GoalReachCount {
            id: 198,
            workplace: 0,
            walk: 9,
        },
        GoalReachCount {
            id: 199,
            workplace: 0,
            walk: 9,
        },
    ];
    assert_eq!(
        report.goal_reaches,
        known.to_vec(),
        "{edition}: map 19 arrivals moved off the pinned counters"
    );
}
