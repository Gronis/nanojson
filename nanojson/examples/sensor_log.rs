//! Embedded-style sensor-log example.
//!
//! Demonstrates a use case typical in constrained environments: parsing a
//! stream of heterogeneous JSON records from a fixed-size input buffer,
//! with no heap allocation at any step.
//!
//! Each record is one of:
//!   {"type":"reading","sensor":"temp","value":2350}   (value in hundredths °C)
//!   {"type":"reading","sensor":"humidity","value":6200}
//!   {"type":"alert","sensor":"temp","code":1}
//!   {"type":"heartbeat"}
//!
//! Shows both API tiers:
//!
//! **`std` tier**: `nanojson::stringify` / `nanojson::parse_as` for
//! quick prototyping without buffer management.
//!
//! **`no_std` tier**: `nanojson::serialize::<N>` + `Parser::new` for the real
//! embedded target — all memory on the stack.

extern crate std;

use nanojson::{Parser};

// ---- Domain types (stack-allocated) ----

#[derive(Debug)]
enum Record {
    Reading { sensor: SensorId, value: i64 },
    Alert   { sensor: SensorId, code:  i64 },
    Heartbeat,
}

#[derive(Debug, PartialEq, Copy, Clone)]
enum SensorId {
    Temp,
    Humidity,
    Unknown,
}

// ---- Parsing helpers ----

/// Copy a &str from a nanojson scratch buffer into a fixed byte array.
/// Call this *immediately* after `object_member()` or `string()` —
/// the next parse call will overwrite the same scratch buffer.
fn copy_str<const N: usize>(s: &str) -> ([u8; N], usize) {
    let mut arr = [0u8; N];
    let len = s.len().min(N);
    arr[..len].copy_from_slice(&s.as_bytes()[..len]);
    (arr, len)
}

fn bstr<const N: usize>(arr: &[u8; N], len: usize) -> &str {
    core::str::from_utf8(&arr[..len]).unwrap_or("")
}

fn parse_sensor_id(s: &str) -> SensorId {
    match s {
        "temp"     => SensorId::Temp,
        "humidity" => SensorId::Humidity,
        _          => SensorId::Unknown,
    }
}

/// Parse one record object. The opening `{` has already been consumed
/// via `array_item()` + `object_begin()` in the caller.
fn parse_record(json: &mut Parser, buf: &mut [u8]) -> Record {
    let mut type_arr  = [0u8; 16]; let mut type_len  = 0usize;
    let mut sensor_arr= [0u8; 16]; let mut sensor_len= 0usize;
    let mut value: Option<i64> = None;
    let mut code:  Option<i64> = None;

    while let Some(key) = json.object_member(buf).unwrap() {
        let (karr, klen) = copy_str::<16>(key);
        match bstr(&karr, klen) {
            "type" => {
                let s = json.string(buf).unwrap();
                let (a, l) = copy_str::<16>(s);
                type_arr = a; type_len = l;
            }
            "sensor" => {
                let s = json.string(buf).unwrap();
                let (a, l) = copy_str::<16>(s);
                sensor_arr = a; sensor_len = l;
            }
            "value" => {
                value = Some(json.number_str().unwrap().parse().unwrap());
            }
            "code" => {
                code = Some(json.number_str().unwrap().parse().unwrap());
            }
            _ => { json.null().ok(); }
        }
    }
    json.object_end().unwrap();

    let type_str   = bstr(&type_arr,   type_len);
    let sensor_str = bstr(&sensor_arr, sensor_len);

    match type_str {
        "reading"   => Record::Reading {
            sensor: parse_sensor_id(sensor_str),
            value:  value.unwrap_or(0),
        },
        "alert"     => Record::Alert {
            sensor: parse_sensor_id(sensor_str),
            code:   code.unwrap_or(0),
        },
        "heartbeat" => Record::Heartbeat,
        other       => panic!("unknown record type: {other}"),
    }
}

fn print_records(records: &[Option<Record>], count: usize) {
    std::println!("Parsed {count} records:");
    for i in 0..count {
        if let Some(r) = &records[i] {
            match r {
                Record::Reading { sensor, value } => {
                    let (int, frac) = (value / 100, value.abs() % 100);
                    std::println!("  [{i}] Reading  {sensor:?}: {int}.{frac:02}");
                }
                Record::Alert { sensor, code } => {
                    std::println!("  [{i}] Alert    {sensor:?}: code {code}");
                }
                Record::Heartbeat => {
                    std::println!("  [{i}] Heartbeat");
                }
            }
        }
    }
}

fn main() {
    // ==================================================================
    // std tier — stringify / parse_as
    // No buffer sizes to choose; heap grows as needed.
    // ==================================================================

    let json = nanojson::stringify_as(|json| {
        json.array_begin()?;

        json.object_begin()?;
          json.member_key("type").unwrap();   json.string("reading")?;
          json.member_key("sensor").unwrap(); json.string("temp")?;
          json.member_key("value").unwrap();  json.integer(2350)?;
        json.object_end()?;

        json.object_begin()?;
          json.member_key("type")?;   json.string("reading")?;
          json.member_key("sensor")?; json.string("humidity")?;
          json.member_key("value")?;  json.integer(6200)?;
        json.object_end()?;

        json.object_begin()?;
          json.member_key("type")?; json.string("heartbeat")?;
        json.object_end()?;

        json.object_begin()?;
          json.member_key("type")?;   json.string("alert")?;
          json.member_key("sensor")?; json.string("temp")?;
          json.member_key("code")?;   json.integer(1)?;
        json.object_end()?;

        json.array_end()
    })
    .unwrap();

    std::println!("=== std tier ===");
    std::println!("Log JSON:\n{}\n", json);

    let mut records: [Option<Record>; 8] = [const { None }; 8];
    let mut count = 0usize;

    nanojson::parse_as(json.as_bytes(), |json, buf| {
        json.array_begin()?;
        while json.array_item()? {
            json.object_begin()?;
            records[count] = Some(parse_record(json, buf));
            count += 1;
        }
        json.array_end()?;
        Ok(())
    })
    .unwrap();

    print_records(&records, count);

    // ==================================================================
    // no_std tier — serialize::<N> + Parser::new
    // All memory on the stack; sizes chosen at compile time.
    // ==================================================================

    std::println!("\n=== no_std tier ===");

    // Build the log into a 512-byte stack buffer.
    let mut buf = [0; 512];
    let log = nanojson::stringify_sized_as(&mut buf, |json| {
        json.array_begin()?;

        json.object_begin()?;
          json.member_key("type")?;   json.string("reading")?;
          json.member_key("sensor")?; json.string("temp")?;
          json.member_key("value")?;  json.integer(2350)?;
        json.object_end()?;

        json.object_begin()?;
          json.member_key("type")?;   json.string("reading")?;
          json.member_key("sensor")?; json.string("humidity")?;
          json.member_key("value")?;  json.integer(6200)?;
        json.object_end()?;

        json.object_begin()?;
          json.member_key("type")?; json.string("heartbeat")?;
        json.object_end()?;

        json.object_begin()?;
          json.member_key("type")?;   json.string("alert")?;
          json.member_key("sensor")?; json.string("temp")?;
          json.member_key("code")?;   json.integer(1)?;
        json.object_end()?;

        json.array_end()
    })
    .unwrap();

    std::println!("Log JSON ({} bytes):\n{}\n", log.len(), log);

    // Parse the log. str_buf = 32 bytes is enough for any single field value.
    let mut records: [Option<Record>; 8] = [const { None }; 8];
    let mut count = 0usize;

    let mut str_buf = [0u8; 32];
    let mut json = Parser::new(log.as_bytes());

    json.array_begin().unwrap();
    while json.array_item().unwrap() {
        json.object_begin().unwrap();
        records[count] = Some(parse_record(&mut json, &mut str_buf));
        count += 1;
    }
    json.array_end().unwrap();

    print_records(&records, count);

    // ==================================================================
    // Size estimation — useful for choosing N on constrained targets
    // ==================================================================

    let n = nanojson::measure(|json| {
        json.object_begin()?;
          json.member_key("type")?;   json.string("reading")?;
          json.member_key("sensor")?; json.string("temp")?;
          json.member_key("value")?;  json.integer(2350)?;
        json.object_end()
    });
    std::println!("\nA 'reading' record is {n} bytes when serialized.");
}
