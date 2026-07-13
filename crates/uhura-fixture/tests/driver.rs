//! Driver behavior (design §9.5): tick scheduling, reply matching and
//! consumption, the two substitutions, revision minting, the closed script
//! grammar, and the TOML converter's refusals.

use serde_json::json;
use uhura_base::to_canonical_json;
use uhura_fixture::{FixtureDriver, toml_to_json};

const FIXTURE: &str = r#"{
  "feed": {
    "page-1": { "has-more": true, "posts": ["post-lena-glaze"] },
    "page-2": { "has-more": false, "posts": ["post-kenji"] },
    "page-1-liked": { "liked": true, "post": "@payload.post", "verbatim": "see @payload.post" },
    "fresh-comment": { "a-id": "@fresh-id", "b-ids": ["@fresh-id"] },
    "needs-payload": { "post": "@payload.post" }
  }
}"#;

fn driver(script: &serde_json::Value) -> FixtureDriver {
    FixtureDriver::new(FIXTURE, &script.to_string()).expect("script should parse")
}

fn new_err(script: &serde_json::Value) -> String {
    FixtureDriver::new(FIXTURE, &script.to_string()).expect_err("script should be rejected")
}

fn like(post: &str, correlation: &str) -> String {
    json!({
        "kind": "command", "port": "feed", "command": "like-post",
        "correlation": correlation, "payload": { "post": post }
    })
    .to_string()
}

fn parse(msg: &str) -> serde_json::Value {
    serde_json::from_str(msg).expect("emitted messages are JSON")
}

// ── tick scheduling and revisions ───────────────────────────────────────

#[test]
fn standalone_delivery_fires_at_its_tick_with_revision_2() {
    let mut d = driver(&json!({
        "deliver": [
            { "after-ticks": 2, "port": "feed", "projection": "feed-page",
              "slice": "feed.page-1" }
        ]
    }));
    assert!(!d.idle());
    assert_eq!(d.tick(), Vec::<String>::new());
    let msgs = d.tick();
    let expected = json!({
        "kind": "projection", "port": "feed", "projection": "feed-page",
        "key": null, "revision": 2,
        "value": { "has-more": true, "posts": ["post-lena-glaze"] }
    });
    // Canonical JSON, byte for byte — sorted keys, compact.
    assert_eq!(msgs, vec![to_canonical_json(&expected)]);
    assert!(d.idle());
}

#[test]
fn same_instance_updates_get_revisions_2_then_3() {
    let mut d = driver(&json!({
        "deliver": [
            { "after-ticks": 1, "port": "feed", "projection": "feed-page",
              "slice": "feed.page-1" },
            { "after-ticks": 2, "port": "feed", "projection": "feed-page",
              "slice": "feed.page-2" },
            { "after-ticks": 2, "port": "feed", "projection": "feed-page",
              "key": "post-kenji", "slice": "feed.page-2" }
        ]
    }));
    assert_eq!(parse(&d.tick()[0])["revision"], 2);
    let msgs = d.tick();
    // Same (projection, key: null) instance advances; a keyed instance has
    // its own counter starting fresh.
    assert_eq!(parse(&msgs[0])["revision"], 3);
    assert_eq!(parse(&msgs[1])["revision"], 2);
}

#[test]
fn projection_failed_carries_no_revision_and_mints_none() {
    let mut d = driver(&json!({
        "deliver": [
            { "after-ticks": 1, "port": "feed", "projection": "feed-page",
              "failed": "unreachable" },
            { "after-ticks": 2, "port": "feed", "projection": "feed-page",
              "slice": "feed.page-1" }
        ]
    }));
    let expected = json!({
        "kind": "projection-failed", "port": "feed", "projection": "feed-page",
        "key": null, "reason": "unreachable"
    });
    assert_eq!(d.tick(), vec![to_canonical_json(&expected)]);
    // The failure did not consume a revision: the next update is still 2.
    assert_eq!(parse(&d.tick()[0])["revision"], 2);
}

#[test]
fn standalone_entries_precede_same_tick_reply_outcomes() {
    let mut d = driver(&json!({
        "deliver": [
            { "after-ticks": 1, "port": "feed", "projection": "feed-page",
              "slice": "feed.page-1" }
        ],
        "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1, "outcome": "ok" }
        ]
    }));
    d.deliver(&like("post-lena-glaze", "c-1")).unwrap();
    let msgs = d.tick();
    assert_eq!(msgs.len(), 2);
    assert_eq!(parse(&msgs[0])["kind"], "projection");
    assert_eq!(parse(&msgs[1])["kind"], "outcome");
}

#[test]
fn after_ticks_counts_from_the_deliver_tick() {
    let mut d = driver(&json!({
        "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 2, "outcome": "ok" }
        ]
    }));
    // Advance to tick 3 first: the outcome is due at 3 + 2 = 5, not 4.
    for _ in 0..3 {
        assert!(d.tick().is_empty());
    }
    d.deliver(&like("post-lena-glaze", "c-1")).unwrap();
    assert!(d.tick().is_empty());
    let msgs = d.tick();
    assert_eq!(parse(&msgs[0])["correlation"], "c-1");
}

#[test]
fn idle_transitions() {
    let mut d = driver(&json!({
        "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1, "outcome": "ok" }
        ]
    }));
    // Unconsumed reply entries do not count — they may never fire.
    assert!(d.idle());
    d.deliver(&like("post-lena-glaze", "c-1")).unwrap();
    assert!(!d.idle());
    assert_eq!(d.tick().len(), 1);
    assert!(d.idle());
}

// ── reply matching ──────────────────────────────────────────────────────

#[test]
fn where_discriminates_and_file_order_wins_among_unconsumed() {
    let script = json!({
        "reply": [
            { "on": { "command": "like-post", "where": { "post": "post-lena-glaze" } },
              "after-ticks": 1, "outcome": "ok" },
            { "on": { "command": "like-post" }, "after-ticks": 1,
              "outcome": "refused", "refusal": "rate-limited" }
        ]
    });
    // A non-matching `where` falls through to the later entry.
    let mut d = driver(&script);
    d.deliver(&like("post-kenji", "c-1")).unwrap();
    let msgs = d.tick();
    assert_eq!(
        parse(&msgs[0])["outcome"],
        json!({ "refused": { "refusal": "rate-limited" } })
    );

    // A matching `where` takes the earlier entry even though both match.
    let mut d = driver(&script);
    d.deliver(&like("post-lena-glaze", "c-2")).unwrap();
    let msgs = d.tick();
    assert_eq!(parse(&msgs[0])["outcome"], json!({ "ok": {} }));
}

#[test]
fn one_shot_entries_are_consumed() {
    let mut d = driver(&json!({
        "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1, "outcome": "ok" }
        ]
    }));
    d.deliver(&like("post-lena-glaze", "c-1")).unwrap();
    // The duplicate in-flight command IS the dedupe assertion (§9.5).
    let err = d.deliver(&like("post-lena-glaze", "c-2")).unwrap_err();
    assert!(err.contains("unscripted command `like-post`"), "{err}");
}

#[test]
fn repeat_entries_match_again() {
    let mut d = driver(&json!({
        "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1, "repeat": true,
              "outcome": "ok" }
        ]
    }));
    d.deliver(&like("post-lena-glaze", "c-1")).unwrap();
    d.deliver(&like("post-kenji", "c-2")).unwrap();
    let msgs = d.tick();
    assert_eq!(parse(&msgs[0])["correlation"], "c-1");
    assert_eq!(parse(&msgs[1])["correlation"], "c-2");
}

#[test]
fn unscripted_command_errors() {
    let mut d = driver(&json!({}));
    let err = d.deliver(&like("post-lena-glaze", "c-1")).unwrap_err();
    assert!(err.contains("on-unscripted"), "{err}");
}

#[test]
fn deliver_rejects_non_command_envelopes() {
    let mut d = driver(&json!({}));
    let projection = json!({
        "kind": "projection", "port": "feed", "projection": "feed-page",
        "key": null, "revision": 2, "value": {}
    });
    let err = d.deliver(&projection.to_string()).unwrap_err();
    assert!(err.contains("command"), "{err}");
}

// ── substitution ────────────────────────────────────────────────────────

#[test]
fn fresh_ids_mint_in_walk_order_from_a_driver_wide_counter() {
    let mut d = driver(&json!({
        "reply": [
            { "on": { "command": "add-comment" }, "after-ticks": 1, "repeat": true,
              "outcome": "ok",
              "updates": [
                  { "port": "comments", "projection": "for-post",
                    "slice": "feed.fresh-comment" }
              ] }
        ]
    }));
    let add = |c: &str| {
        json!({
            "kind": "command", "port": "comments", "command": "add-comment",
            "correlation": c, "payload": {}
        })
        .to_string()
    };
    d.deliver(&add("c-1")).unwrap();
    let value = parse(&d.tick()[0])["updates"][0]["value"].clone();
    assert_eq!(value, json!({ "a-id": "fresh-1", "b-ids": ["fresh-2"] }));

    // Per-driver counter: the next emission keeps counting.
    d.deliver(&add("c-2")).unwrap();
    let value = parse(&d.tick()[0])["updates"][0]["value"].clone();
    assert_eq!(value, json!({ "a-id": "fresh-3", "b-ids": ["fresh-4"] }));
}

#[test]
fn payload_markers_substitute_whole_strings_only() {
    let mut d = driver(&json!({
        "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1, "outcome": "ok",
              "updates": [
                  { "port": "feed", "projection": "feed-page",
                    "slice": "feed.page-1-liked" }
              ] }
        ]
    }));
    d.deliver(&like("post-lena-glaze", "c-1")).unwrap();
    let value = parse(&d.tick()[0])["updates"][0]["value"].clone();
    assert_eq!(value["post"], "post-lena-glaze");
    // Not a whole-string match — passes through verbatim.
    assert_eq!(value["verbatim"], "see @payload.post");
}

#[test]
fn from_payload_keys_resolve_at_deliver_time() {
    let mut d = driver(&json!({
        "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1, "outcome": "ok",
              "updates": [
                  { "port": "feed", "projection": "for-post",
                    "key": { "from": "payload.post" }, "slice": "feed.page-1" }
              ] }
        ]
    }));
    d.deliver(&like("post-lena-glaze", "c-1")).unwrap();
    let update = parse(&d.tick()[0])["updates"][0].clone();
    assert_eq!(update["key"], "post-lena-glaze");
    assert_eq!(update["revision"], 2);
}

#[test]
fn missing_payload_field_fails_the_deliver_call() {
    let script = json!({
        "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1, "outcome": "ok",
              "updates": [
                  { "port": "feed", "projection": "feed-page",
                    "slice": "feed.needs-payload" }
              ] }
        ]
    });
    let cmd = json!({
        "kind": "command", "port": "feed", "command": "like-post",
        "correlation": "c-1", "payload": {}
    })
    .to_string();
    let mut d = driver(&script);
    let err = d.deliver(&cmd).unwrap_err();
    assert!(err.contains("no `post` field"), "{err}");
    assert!(d.idle(), "a failed deliver schedules nothing");

    // Same for a `{ "from": "payload.<field>" }` key.
    let script = json!({
        "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1, "outcome": "ok",
              "updates": [
                  { "port": "feed", "projection": "for-post",
                    "key": { "from": "payload.post" }, "slice": "feed.page-1" }
              ] }
        ]
    });
    let err = driver(&script).deliver(&cmd).unwrap_err();
    assert!(err.contains("no `post` field"), "{err}");
}

// ── outcome shapes ──────────────────────────────────────────────────────

#[test]
fn outcomes_serialize_to_the_envelope_shapes() {
    let mut d = driver(&json!({
        "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1,
              "outcome": "refused", "refusal": "rate-limited" },
            { "on": { "command": "load-next-page" }, "after-ticks": 1,
              "outcome": "unavailable", "reason": "unreachable" }
        ]
    }));
    d.deliver(&like("post-lena-glaze", "c-1")).unwrap();
    d.deliver(
        &json!({
            "kind": "command", "port": "feed", "command": "load-next-page",
            "correlation": "c-2", "payload": {}
        })
        .to_string(),
    )
    .unwrap();
    let msgs = d.tick();
    let refused = json!({
        "kind": "outcome", "correlation": "c-1",
        "outcome": { "refused": { "refusal": "rate-limited" } }, "updates": []
    });
    let unavailable = json!({
        "kind": "outcome", "correlation": "c-2",
        "outcome": { "unavailable": { "reason": "unreachable" } }, "updates": []
    });
    assert_eq!(
        msgs,
        vec![to_canonical_json(&refused), to_canonical_json(&unavailable)]
    );
}

// ── the closed grammar (rejections at new) ──────────────────────────────

#[test]
fn rejects_unknown_keys_at_every_level() {
    let cases = [
        json!({ "scripts": [] }),
        json!({ "deliver": [
            { "after-ticks": 1, "port": "feed", "projection": "feed-page",
              "slice": "feed.page-1", "extra": 1 } ] }),
        json!({ "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1,
              "outcome": "ok", "extra": 1 } ] }),
        json!({ "reply": [
            { "on": { "command": "like-post", "port": "feed" }, "after-ticks": 1,
              "outcome": "ok" } ] }),
        json!({ "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1, "outcome": "ok",
              "updates": [ { "port": "feed", "projection": "feed-page",
                             "slice": "feed.page-1", "failed": "x" } ] } ] }),
    ];
    for script in &cases {
        let err = new_err(script);
        assert!(err.contains("grammar is closed"), "{script}: {err}");
    }
}

#[test]
fn rejects_any_on_unscripted_but_error() {
    let err = new_err(&json!({ "on-unscripted": "ignore" }));
    assert!(err.contains("the only policy"), "{err}");
    driver(&json!({ "on-unscripted": "error" }));
}

#[test]
fn accepts_and_ignores_the_harness_only_ui_key() {
    let d = driver(&json!({ "ui": [ { "anything": ["at", "all"] } ] }));
    assert!(d.idle());
}

#[test]
fn rejects_after_ticks_below_1() {
    for bad in [json!(0), json!(-1), json!("1"), json!(null)] {
        let err = new_err(&json!({ "deliver": [
            { "after-ticks": bad, "port": "feed", "projection": "feed-page",
              "slice": "feed.page-1" } ] }));
        assert!(err.contains("after-ticks"), "{err}");
    }
    let err = new_err(&json!({ "reply": [
        { "on": { "command": "like-post" }, "after-ticks": 0, "outcome": "ok" } ] }));
    assert!(err.contains("after-ticks"), "{err}");
}

#[test]
fn rejects_deliver_without_exactly_one_of_slice_or_failed() {
    for entry in [
        json!({ "after-ticks": 1, "port": "feed", "projection": "feed-page" }),
        json!({ "after-ticks": 1, "port": "feed", "projection": "feed-page",
                "slice": "feed.page-1", "failed": "unreachable" }),
    ] {
        let err = new_err(&json!({ "deliver": [entry] }));
        assert!(err.contains("exactly one of"), "{err}");
    }
}

#[test]
fn rejects_dangling_slice_references() {
    let err = new_err(&json!({ "deliver": [
        { "after-ticks": 1, "port": "feed", "projection": "feed-page",
          "slice": "feed.page-9" } ] }));
    assert!(err.contains("no fixture slice `feed.page-9`"), "{err}");

    let err = new_err(&json!({ "reply": [
        { "on": { "command": "like-post" }, "after-ticks": 1, "outcome": "ok",
          "updates": [ { "port": "feed", "projection": "feed-page",
                         "slice": "nope.nothing" } ] } ] }));
    assert!(err.contains("no fixture slice `nope.nothing`"), "{err}");
}

#[test]
fn rejects_payload_markers_in_standalone_slices_at_new() {
    let err = new_err(&json!({ "deliver": [
        { "after-ticks": 1, "port": "feed", "projection": "feed-page",
          "slice": "feed.needs-payload" } ] }));
    assert!(err.contains("reply-only"), "{err}");
}

#[test]
fn rejects_mismatched_outcome_details() {
    let entry = |patch: serde_json::Value| {
        let mut entry = json!({ "on": { "command": "like-post" }, "after-ticks": 1 });
        let obj = entry.as_object_mut().unwrap();
        for (k, v) in patch.as_object().unwrap() {
            obj.insert(k.clone(), v.clone());
        }
        json!({ "reply": [entry] })
    };
    for patch in [
        json!({ "outcome": "ok", "refusal": "rate-limited" }),
        json!({ "outcome": "ok", "reason": "unreachable" }),
        json!({ "outcome": "refused" }),
        json!({ "outcome": "refused", "refusal": "rate-limited", "reason": "x" }),
        json!({ "outcome": "unavailable" }),
        json!({ "outcome": "unavailable", "reason": "unreachable", "refusal": "x" }),
        json!({ "outcome": "exploded" }),
    ] {
        new_err(&entry(patch));
    }
}

#[test]
fn rejects_computed_keys_where_a_literal_is_required() {
    // Deliver keys are literals.
    let err = new_err(&json!({ "deliver": [
        { "after-ticks": 1, "port": "feed", "projection": "feed-page",
          "key": { "from": "payload.post" }, "slice": "feed.page-1" } ] }));
    assert!(err.contains("reply-only"), "{err}");

    // Reply keys accept only the exact `{ "from": "payload.<field>" }` shape.
    for bad in [
        json!({ "from": "payload.post", "extra": 1 }),
        json!({ "from": "post" }),
        json!({ "from": "payload." }),
        json!({ "from": 3 }),
    ] {
        let err = new_err(&json!({ "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1, "outcome": "ok",
              "updates": [ { "port": "feed", "projection": "for-post",
                             "key": bad, "slice": "feed.page-1" } ] } ] }));
        assert!(err.contains("payload.<field>"), "{err}");
    }
}

#[test]
fn rejects_floats_everywhere() {
    let err = FixtureDriver::new(r#"{ "feed": { "bad": 1.5 } }"#, "{}").unwrap_err();
    assert!(err.contains("floats"), "{err}");

    let err = FixtureDriver::new(FIXTURE, r#"{ "reply": [ { "on": { "command": "like-post", "where": { "x": 1.5 } }, "after-ticks": 1, "outcome": "ok" } ] }"#)
        .unwrap_err();
    assert!(err.contains("floats"), "{err}");

    let mut d = driver(&json!({}));
    let cmd = r#"{ "kind": "command", "port": "feed", "command": "like-post",
                   "correlation": "c-1", "payload": { "x": 1.5 } }"#;
    let err = d.deliver(cmd).unwrap_err();
    assert!(err.contains("floats"), "{err}");
}

#[test]
fn rejects_malformed_fixture_shapes() {
    let err = FixtureDriver::new("[]", "{}").unwrap_err();
    assert!(err.contains("<ns>"), "{err}");
    let err = FixtureDriver::new(r#"{ "feed": [] }"#, "{}").unwrap_err();
    assert!(err.contains("object of slices"), "{err}");
}

// ── toml_to_json ────────────────────────────────────────────────────────

#[test]
fn toml_converts_the_value_model() {
    let json = toml_to_json(
        r#"
count = 3
open = true

[names]
first = "lena"
tags = ["a", "b"]
"#,
    )
    .unwrap();
    assert_eq!(
        json,
        json!({
            "count": 3, "open": true,
            "names": { "first": "lena", "tags": ["a", "b"] }
        })
    );
}

#[test]
fn toml_refuses_floats_and_datetimes() {
    assert_eq!(
        toml_to_json("x = 1.5").unwrap_err(),
        "floats do not exist in fixture data (§7.5)"
    );
    assert_eq!(
        toml_to_json("when = 1979-05-27T07:32:00Z").unwrap_err(),
        "no clocks: time labels are provider-formatted text (§9.1)"
    );
    assert_eq!(
        toml_to_json("day = 1979-05-27").unwrap_err(),
        "no clocks: time labels are provider-formatted text (§9.1)"
    );
}

#[test]
fn payload_data_equal_to_a_marker_string_echoes_verbatim() {
    // §9.5: payload echo is VERBATIM — wire data that happens to spell
    // "@fresh-id" (or another marker) must not be re-interpreted, and the
    // mint counter must not advance on it.
    let mut d = driver(&json!({
        "reply": [
            { "on": { "command": "like-post" }, "after-ticks": 1, "outcome": "ok",
              "updates": [ { "port": "feed", "projection": "feed-page",
                             "slice": "feed.needs-payload" } ] },
            { "on": { "command": "like-post" }, "after-ticks": 1, "outcome": "ok",
              "updates": [ { "port": "feed", "projection": "feed-page",
                             "slice": "feed.fresh-comment" } ] }
        ]
    }));
    d.deliver(&like("@fresh-id", "c-1")).expect("scripted");
    let msgs = d.tick();
    let outcome = parse(&msgs[0]);
    assert_eq!(
        outcome["updates"][0]["value"],
        json!({ "post": "@fresh-id" }),
        "spliced payload data is untouched"
    );
    // The next authored marker still mints from 1 — wire data never
    // advanced the counter.
    d.deliver(&like("post-x", "c-2")).expect("scripted");
    let msgs = d.tick();
    let outcome = parse(&msgs[0]);
    assert_eq!(
        outcome["updates"][0]["value"],
        json!({ "a-id": "fresh-1", "b-ids": ["fresh-2"] })
    );
}
