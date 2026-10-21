use ghost_proto::*;

#[test]
fn hello_exact_wire_form() {
    assert_eq!(
        encode_line(&ServerMsg::Hello {
            proto: 1,
            host: "tkacz-workstation".into()
        }),
        "{\"t\":\"hello\",\"proto\":1,\"host\":\"tkacz-workstation\"}\n"
    );
}
#[test]
fn data_envelope_wraps_snapshot() {
    let v: serde_json::Value = serde_json::from_str(&encode_line(&data_msg(
        topics::GPU,
        3,
        &GpuSnapshot {
            present: true,
            ..Default::default()
        },
    )))
    .unwrap();
    assert_eq!(v["t"], "data");
    assert_eq!(v["topic"], "gpu");
    assert_eq!(v["seq"], 3);
    assert_eq!(v["data"]["present"], true);
}
#[test]
fn provider_messages_decode() {
    assert_eq!(
        decode_client_line(
            r#"{"t":"provide","proto":1,"name":"inmet","topics":["weather.alerts"]}"#
        )
        .unwrap(),
        ClientMsg::Provide {
            proto: 1,
            name: "inmet".into(),
            topics: vec!["weather.alerts".into()]
        }
    );
    assert!(matches!(
        decode_client_line(r#"{"t":"publish","topic":"weather.alerts","data":{"n":1}}"#).unwrap(),
        ClientMsg::Publish { .. }
    ));
}
#[test]
fn topic_names() {
    assert!(is_valid_topic("weather.alerts"));
    assert!(is_valid_topic("cpu"));
    assert!(!is_valid_topic("Weather"));
    assert!(!is_valid_topic("a..b"));
    assert!(!is_valid_topic(&"a".repeat(65)));
}
#[test]
fn unavailable_reading_is_null() {
    assert_eq!(
        serde_json::to_value(Reading {
            value: None,
            ..Default::default()
        })
        .unwrap()["value"],
        serde_json::Value::Null
    );
}
#[test]
fn drive_state_wire_names() {
    assert_eq!(
        serde_json::to_string(&DriveState::Dead).unwrap(),
        "\"dead\""
    );
}
#[test]
fn a_missing_history_sample_round_trips_as_null() {
    let history = ServerMsg::History {
        step_ms: 1000,
        series: [("load".to_owned(), vec![Some(12.5), None, Some(40.0)])].into(),
    };

    let line = encode_line(&history);

    assert!(line.contains("\"load\":[12.5,null,40.0]"), "{line}");
    assert_eq!(serde_json::from_str::<ServerMsg>(&line).unwrap(), history);
}
#[test]
fn rejects_garbage() {
    assert!(decode_client_line("{nope").is_err());
}
#[test]
fn desktop_snapshot_wire_form() {
    let snapshot = DesktopSnapshot {
        covered_outputs: vec!["DP-1".into()],
        active: true,
        action: Some(ViewAction::Monitor),
        action_seq: 1,
    };

    assert_eq!(
        serde_json::to_string(&snapshot).unwrap(),
        r#"{"covered_outputs":["DP-1"],"active":true,"action":"monitor","action_seq":1}"#
    );
    assert_eq!(
        serde_json::to_value(DesktopSnapshot::default()).unwrap()["action"],
        serde_json::Value::Null
    );
    assert_eq!(
        serde_json::to_value(ViewAction::Swap).unwrap(),
        serde_json::json!("swap")
    );
}
#[test]
fn weather_snapshot_wire_form() {
    let snapshot = WeatherSnapshot {
        available: true,
        stale: false,
        updated_s: 1792610000,
        temp_c: 18.0,
        code: 95,
        condition: "THUNDERSTORM".into(),
        is_day: false,
        humidity_pct: 88,
        wind_kmh: 31.0,
        high_c: 22.0,
        low_c: 14.0,
        hours: vec![WeatherHour {
            at_s: 1792612800,
            temp_c: 17.5,
            rain_pct: 95,
        }],
        rain_peak: Some(RainPeak {
            pct: 95,
            at_s: 1792612800,
        }),
        aqi: Some(42),
    };

    assert_eq!(
        serde_json::to_string(&snapshot).unwrap(),
        concat!(
            r#"{"available":true,"stale":false,"updated_s":1792610000,"temp_c":18.0,"code":95,"#,
            r#""condition":"THUNDERSTORM","is_day":false,"humidity_pct":88,"wind_kmh":31.0,"#,
            r#""high_c":22.0,"low_c":14.0,"hours":[{"at_s":1792612800,"temp_c":17.5,"#,
            r#""rain_pct":95}],"rain_peak":{"pct":95,"at_s":1792612800},"aqi":42}"#
        )
    );
    assert_eq!(
        serde_json::to_string(&WeatherSnapshot::default()).unwrap(),
        concat!(
            r#"{"available":false,"stale":false,"updated_s":0,"temp_c":0.0,"code":0,"#,
            r#""condition":"","is_day":false,"humidity_pct":0,"wind_kmh":0.0,"high_c":0.0,"#,
            r#""low_c":0.0,"hours":[],"rain_peak":null,"aqi":null}"#
        )
    );
}

#[test]
fn sky_snapshot_wire_form() {
    let snapshot = SkySnapshot {
        available: true,
        sunrise_s: Some(1792570620),
        sunset_s: Some(1792619520),
        moon_phase: 0.296,
        moon_illumination: 0.64,
        moon_name: "WAXING GIBBOUS".into(),
    };

    assert_eq!(
        serde_json::to_string(&snapshot).unwrap(),
        concat!(
            r#"{"available":true,"sunrise_s":1792570620,"sunset_s":1792619520,"#,
            r#""moon_phase":0.296,"moon_illumination":0.64,"moon_name":"WAXING GIBBOUS"}"#
        )
    );
    assert_eq!(
        serde_json::to_string(&SkySnapshot::default()).unwrap(),
        concat!(
            r#"{"available":false,"sunrise_s":null,"sunset_s":null,"moon_phase":0.0,"#,
            r#""moon_illumination":0.0,"moon_name":""}"#
        )
    );
}
#[test]
fn weather_alerts_wire_form() {
    let alerts = WeatherAlerts {
        source: "INMET".into(),
        updated_s: 1792610000,
        alerts: vec![WeatherAlert {
            id: "56134".into(),
            title: "TEMPESTADE".into(),
            severity: "PERIGO POTENCIAL".into(),
            original_title: None,
            original_severity: None,
            auto_translated: false,
            level: AlertLevel::Warn,
            start_s: 1792616400,
            end_s: 1792659600,
            risks: vec!["Chuva entre 20 e 30 mm/h.".into()],
        }],
    };

    assert_eq!(
        serde_json::to_string(&alerts).unwrap(),
        concat!(
            r#"{"source":"INMET","updated_s":1792610000,"alerts":[{"id":"56134","#,
            r#""title":"TEMPESTADE","severity":"PERIGO POTENCIAL","original_title":null,"#,
            r#""original_severity":null,"auto_translated":false,"level":"warn","#,
            r#""start_s":1792616400,"end_s":1792659600,"risks":["Chuva entre 20 e 30 mm/h."]}]}"#
        )
    );
    let translated = WeatherAlert {
        title: "RAIN ACCUMULATION".into(),
        severity: "POTENTIAL DANGER".into(),
        original_title: Some("ACUMULADO DE CHUVA".into()),
        original_severity: Some("PERIGO POTENCIAL".into()),
        auto_translated: true,
        ..alerts.alerts[0].clone()
    };
    assert_eq!(
        serde_json::to_string(&translated).unwrap(),
        concat!(
            r#"{"id":"56134","title":"RAIN ACCUMULATION","severity":"POTENTIAL DANGER","#,
            r#""original_title":"ACUMULADO DE CHUVA","original_severity":"PERIGO POTENCIAL","#,
            r#""auto_translated":true,"level":"warn","start_s":1792616400,"end_s":1792659600,"#,
            r#""risks":["Chuva entre 20 e 30 mm/h."]}"#
        )
    );
    assert_eq!(
        serde_json::to_value(AlertLevel::Crit).unwrap(),
        serde_json::json!("crit")
    );
}
#[test]
fn topic_state_exact_wire_form() {
    for (live, wire) in [
        (
            true,
            "{\"t\":\"topic_state\",\"topic\":\"weather.alerts\",\"live\":true}\n",
        ),
        (
            false,
            "{\"t\":\"topic_state\",\"topic\":\"weather.alerts\",\"live\":false}\n",
        ),
    ] {
        let state = ServerMsg::TopicState {
            topic: topics::WEATHER_ALERTS.into(),
            live,
        };

        let line = encode_line(&state);

        assert_eq!(line, wire);
        assert_eq!(serde_json::from_str::<ServerMsg>(&line).unwrap(), state);
    }
}
#[test]
fn spectrum_snapshot_wire_form() {
    let snapshot = SpectrumSnapshot {
        bands: vec![0, 128, 255],
    };

    assert_eq!(
        serde_json::to_string(&snapshot).unwrap(),
        r#"{"bands":[0,128,255]}"#
    );
    assert_eq!(
        serde_json::to_string(&SpectrumSnapshot::default()).unwrap(),
        r#"{"bands":[]}"#
    );
}
