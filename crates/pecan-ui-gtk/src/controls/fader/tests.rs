//! Synthetic numbers and opaque local tokens only; no display/audio/profile.
use super::*;
use serde_json::{json, Value};
use std::rc::Rc;

fn rendered(outputs: Vec<Output<u64>>) -> Vec<Value> {
    outputs
        .into_iter()
        .map(|out| match out {
            Output::Begin { token, kind } => json!({"op":"begin", "token":token, "kind": match kind {
                Kind::Pointer=>"pointer",Kind::Wheel=>"wheel",Kind::Reset=>"reset"
            }}),
            Output::Update { token, db } => json!({"op":"update","token":token,"db":db}),
            Output::End { token, db, reason } => json!({"op":"end","token":token,"db":db,"reason":match reason {
                End::Released=>"released",End::PointerCancelled=>"pointer_cancelled",End::WheelIdle=>"wheel_idle",End::Reset=>"reset"
            }}),
            Output::Abandon { token } => json!({"op":"abandon","token":token}),
        })
        .collect()
}
fn same(actual: &Value, expected: &Value, name: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-10,
            "{name}: {actual} != {expected}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{name}: {actual} != {expected}");
            for (a, b) in a.iter().zip(b) {
                same(a, b, name);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{name}");
            for (key, value) in a {
                same(value, b.get(key).expect(key), name);
            }
        }
        _ => assert_eq!(actual, expected, "{name}"),
    }
}
#[test]
fn unchanged_js_callbacks_match_all_committed_synthetic_traces() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    let traces = oracle["scenarios"].as_array().unwrap();
    assert_eq!(traces.len(), 83);
    for trace in traces {
        let name = trace["name"].as_str().unwrap();
        let mut fader = Fader::new(trace["initial_db"].as_f64().unwrap()).unwrap();
        let mut actual = vec![];
        let mut now = Duration::ZERO;
        for step in trace["steps"].as_array().unwrap() {
            if let Some(ms) = step["at_ms"].as_u64() {
                now = Duration::from_millis(ms);
            }
            let outputs = match step["op"].as_str().unwrap() {
                "observe" => {
                    fader.observe(step["db"].as_f64().unwrap()).unwrap();
                    vec![]
                }
                "down" => fader
                    .pointer_down(
                        step["token"].as_u64().unwrap(),
                        if step["button"].as_u64().unwrap_or(0) == 0 {
                            PointerButton::Primary
                        } else {
                            PointerButton::Other
                        },
                        step["y"].as_f64().unwrap(),
                    )
                    .unwrap(),
                "move" => fader
                    .pointer_move(
                        step["y"].as_f64().unwrap(),
                        step["height"].as_f64(),
                        step["shift"].as_bool().unwrap_or(false),
                    )
                    .unwrap(),
                "up" => fader.pointer_up(),
                "cancel" => fader.pointer_cancel(),
                "wheel" => fader
                    .wheel(
                        step["token"].as_u64().unwrap(),
                        step["delta_y"].as_f64().unwrap(),
                        step["shift"].as_bool().unwrap_or(false),
                        now,
                    )
                    .unwrap(),
                "reset" => fader.double_click(step["token"].as_u64().unwrap()),
                "advance" => fader.advance(now).unwrap(),
                op => panic!("Unknown fixture op: {op}"),
            };
            assert!(outputs.len() <= 3);
            actual.extend(rendered(outputs));
        }
        same(&json!(actual), &trace["expected"], name);
        same(&json!(fader.position()), &trace["position"], name);
        same(&json!(fader.observed_db()), &trace["observed_db"], name);
        assert_eq!(
            fader.dragging(),
            trace["dragging"].as_bool().unwrap(),
            "{name}"
        );
        same(
            &json!(fader.wheel_deadline().map(|d| d.as_millis())),
            &trace["wheel_deadline_ms"],
            name,
        );
    }
}
#[test]
fn rounding_uses_js_ties_and_negative_zero_instead_of_rust_round() {
    let oracle: Value = serde_json::from_str(include_str!("oracle.json")).unwrap();
    for case in oracle["rounding"].as_array().unwrap() {
        // Preserve the exact oracle float on rounding boundaries. serde_json's
        // default number parser can round the float below 0.05 up to 0.05.
        let input =
            f64::from_bits(u64::from_str_radix(case["input_bits"].as_str().unwrap(), 16).unwrap());
        let result = round_tenth(input);
        assert_eq!(result, case["expected"].as_f64().unwrap(), "{input}");
        assert_eq!(
            result == 0.0 && result.is_sign_negative(),
            case["negative_zero"].as_bool().unwrap(),
            "{input}"
        );
    }
    assert_eq!(round_tenth(-0.05), -0.0);
    assert_eq!(round_tenth(0.049999999999999996), 0.0);
    assert_eq!(round_tenth(-6.05), -6.0);
}
#[test]
fn motion_storm_keeps_one_original_token_and_no_proposal_queue() {
    let token = Rc::new(());
    let mut fader = Fader::new(-10.0).unwrap();
    drop(
        fader
            .pointer_down(token.clone(), PointerButton::Primary, 0.0)
            .unwrap(),
    );
    for i in 0..100_000 {
        let outputs = fader
            .pointer_move(f64::from(i % 500), Some(160.0), i % 3 == 0)
            .unwrap();
        assert_eq!(outputs.len(), 1);
        assert!(
            matches!(&outputs[0],Output::Update{token:held,db} if Rc::ptr_eq(held,&token)&&db.is_finite())
        );
        drop(outputs);
        assert_eq!(Rc::strong_count(&token), 2);
    }
    assert!(fader.dragging());
    assert_eq!(fader.pointer_up().len(), 1);
    assert_eq!(Rc::strong_count(&token), 1);
    assert!(fader.pointer_up().is_empty());
}
#[test]
fn wheel_storm_retains_first_token_and_only_one_latest_deadline() {
    let token = Rc::new(0);
    let mut fader = Fader::new(-10.0).unwrap();
    drop(
        fader
            .wheel(token.clone(), -1.0, false, Duration::ZERO)
            .unwrap(),
    );
    for i in 1..100_000 {
        let incoming = Rc::new(i);
        let weak = Rc::downgrade(&incoming);
        let outputs = fader
            .wheel(incoming, -1.0, false, Duration::from_micros(i as u64))
            .unwrap();
        assert_eq!(outputs.len(), 1);
        assert!(
            matches!(&outputs[0],Output::Update{token:held,db:-9.0} if Rc::ptr_eq(held,&token))
        );
        drop(outputs);
        assert!(
            weak.upgrade().is_none(),
            "new callback cannot replace burst identity"
        );
        assert_eq!(Rc::strong_count(&token), 2);
    }
    let deadline = fader.wheel_deadline().unwrap();
    assert!(fader
        .advance(deadline - Duration::from_nanos(1))
        .unwrap()
        .is_empty());
    assert_eq!(fader.advance(deadline).unwrap().len(), 1);
    assert!(fader.wheel_deadline().is_none());
    assert_eq!(Rc::strong_count(&token), 1);
}
#[test]
fn owner_invalidation_abandons_both_slots_without_a_fake_release_or_rollback() {
    let mut fader = Fader::new(-10.0).unwrap();
    fader.pointer_down(1, PointerButton::Primary, 0.0).unwrap();
    fader.pointer_move(-20.0, Some(160.0), false).unwrap();
    fader.wheel(2, -1.0, false, Duration::ZERO).unwrap();
    assert_eq!(
        fader.invalidate(),
        vec![Output::Abandon { token: 1 }, Output::Abandon { token: 2 }]
    );
    assert!(fader.pointer_cancel().is_empty());
    assert!(fader.pointer_up().is_empty());
    assert!(fader
        .pointer_move(10.0, Some(160.0), false)
        .unwrap()
        .is_empty());
    assert!(fader.advance(Duration::from_secs(6)).unwrap().is_empty());
    fader.observe(-20.0).unwrap();
    assert_eq!(
        fader.pointer_down(3, PointerButton::Primary, 0.0).unwrap(),
        vec![Output::Begin {
            token: 3,
            kind: Kind::Pointer
        }]
    );
}
#[test]
fn invalid_input_is_rejected_without_changing_valid_gesture_state() {
    assert!(matches!(Fader::<()>::new(f64::NAN), Err(Error::NonFinite)));
    let mut fader = Fader::new(-10.0).unwrap();
    assert!(fader
        .pointer_down(1, PointerButton::Primary, f64::INFINITY)
        .is_err());
    assert!(!fader.dragging());
    fader
        .pointer_down(1, PointerButton::Primary, 100.0)
        .unwrap();
    assert_eq!(
        fader.pointer_down(2, PointerButton::Primary, 0.0),
        Err(Error::AlreadyDragging)
    );
    for height in [0.0, 24.0, -10.0] {
        assert_eq!(
            fader.pointer_move(50.0, Some(height), false),
            Err(Error::InvalidHeight)
        );
    }
    assert_eq!(
        fader.pointer_move(f64::NAN, Some(160.0), false),
        Err(Error::NonFinite)
    );
    assert_eq!(fader.observe(f64::NEG_INFINITY), Err(Error::NonFinite));
    assert_eq!(fader.position(), db_to_position(-10.0));
    assert_eq!(fader.observed_db(), -10.0);
    fader.pointer_move(100.0, Some(160.0), false).unwrap();
    assert_eq!(
        fader.position(),
        db_to_position(-10.0),
        "bad moves must not consume relative y"
    );
}
#[test]
fn backward_clock_and_bad_wheel_input_do_not_replace_the_pending_timer() {
    let mut fader = Fader::new(-10.0).unwrap();
    let now = Duration::from_secs(2);
    fader.wheel(1, -1.0, false, now).unwrap();
    let deadline = fader.wheel_deadline();
    assert_eq!(
        fader.wheel(2, -1.0, false, Duration::ZERO),
        Err(Error::ClockRegressed)
    );
    assert_eq!(fader.advance(Duration::ZERO), Err(Error::ClockRegressed));
    assert_eq!(
        fader.wheel(2, f64::NAN, false, now + Duration::from_secs(5)),
        Err(Error::NonFinite)
    );
    assert_eq!(fader.wheel_deadline(), deadline);
    assert_eq!(
        fader.advance(now + WHEEL_IDLE).unwrap(),
        vec![Output::End {
            token: 1,
            db: -9.0,
            reason: End::WheelIdle
        }]
    );
}
#[test]
fn pointer_cancel_is_an_end_but_never_invents_an_update() {
    let mut fader = Fader::new(-7.123).unwrap();
    fader
        .pointer_down(1, PointerButton::Primary, 500.0)
        .unwrap();
    assert_eq!(
        fader.pointer_cancel(),
        vec![Output::End {
            token: 1,
            db: -7.1,
            reason: End::PointerCancelled
        }]
    );
    assert_eq!(fader.observed_db(), -7.123);
    assert!(fader.pointer_up().is_empty());
}
