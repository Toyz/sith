//! Queries against the built-in Win16 ordinal database.

use crate::style::*;
use anyhow::Result;
use ne_core::{ApiDb, OrdinalDb};

/// Print the signature and any named parameter sets for one entry point.
fn print_signature(module: &str, ordinal: u16) {
    let api = ApiDb::embedded();
    let Some(sig) = api.signature(module, ordinal) else {
        println!("{}", dim("  no signature in the Win16 API table"));
        return;
    };
    println!("  {} {}", dim("signature"), sig.render());
    println!(
        "  {} {:?}, {} stack words",
        dim("calling  "),
        sig.conv,
        sig.stack_words()
    );
    for (i, arg) in sig.args.iter().enumerate() {
        let label = sig.param_name(i).unwrap_or("");
        let head = format!("    {:<12} {:<8}", cyan(label), arg.as_str());
        match api.param_set(module, &sig.name, i) {
            Some(s) => {
                println!("{head} {}", magenta(&s.name));
                for (v, n) in s.values.iter().take(64) {
                    println!("        {v:#010X}  {n}");
                }
            }
            None => println!("{head}"),
        }
    }
}

pub fn run(module: Option<&str>, ordinal: Option<u16>) -> Result<()> {
    let db = OrdinalDb::embedded();
    match (module, ordinal) {
        (None, _) => {
            let mut mods: Vec<&str> = db.modules().collect();
            mods.sort();
            println!(
                "{}",
                heading(&format!(
                    "{} names across {} modules",
                    db.len(),
                    db.module_count()
                ))
            );
            for m in mods {
                println!("  {}", magenta(m));
            }
        }
        (Some(m), Some(o)) => match db.lookup(m, o) {
            Some(n) => {
                println!("{m}.{o} = {}", cyan(n));
                print_signature(m, o);
            }
            None => println!("{}", dim(&format!("{m}.{o} is not in the table"))),
        },
        (Some(m), None) => {
            // The database is keyed by module and ordinal, so listing one
            // module means probing its ordinal space; 0xFFFF is the ceiling.
            let mut rows = Vec::new();
            for o in 0..=u16::MAX {
                if let Some(n) = db.lookup(m, o) {
                    rows.push((o, n));
                }
            }
            if rows.is_empty() {
                println!("{}", dim(&format!("no ordinals known for {m}")));
                return Ok(());
            }
            println!("{}", heading(&format!("{m} ({} ordinals)", rows.len())));
            let api = ApiDb::embedded();
            for (o, n) in rows {
                match api.signature(m, o) {
                    Some(sig) => println!("  @{:<6} {}", o, cyan(&sig.render())),
                    None => println!("  @{:<6} {}", o, cyan(n)),
                }
            }
        }
    }
    Ok(())
}
