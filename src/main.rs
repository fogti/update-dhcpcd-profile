use clap::Arg;
use std::collections::{HashMap, HashSet};
use std::io::{BufWriter, Write};
use std::process::Command;

fn get_dump(iface: &str) -> Result<HashMap<String, String>, String> {
    let dhdump = Command::new("dhcpcd")
        .args(&["-U", iface])
        .output()
        .expect("failed to spawn 'dhcpcd -U'");

    let mut vars: HashMap<_, _> = std::str::from_utf8(&dhdump.stdout)
        .expect("got non-utf8 result")
        .lines()
        .flat_map(|i| i.find(|x| x == '=').map(|pos| (&i[..pos], &i[pos + 1..])))
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| {
            (
                key.to_owned(),
                if value.bytes().nth(0).unwrap() == b'\''
                    && value.bytes().rev().nth(0).unwrap() == b'\''
                {
                    &value[1..value.len() - 1]
                } else {
                    value
                }
                .to_owned(),
            )
        })
        .collect();

    if vars.is_empty() {
        return Err(std::str::from_utf8(&dhdump.stderr)
            .expect("got non-utf8 result")
            .to_owned());
    }

    let selected_vars: HashSet<&str> = [
        "broadcast_address",
        "domain_name_servers",
        "ip_address",
        "routers",
        "subnet_cidr",
    ]
    .iter()
    .copied()
    .collect();

    vars.retain(|var, _| selected_vars.contains(&var[..]));
    Ok(vars)
}

fn read_config_from_file(fpath: &str) -> Result<Vec<String>, anyhow::Error> {
    Ok(
        std::str::from_utf8(&*readfilez::read_from_file(std::fs::File::open(fpath))?)?
            .lines()
            .map(|i| i.to_owned())
            .collect(),
    )
}

fn write_config_to_file(config: &[String], fpath: &str) -> Result<(), anyhow::Error> {
    let fpath = std::path::Path::new(fpath);
    let mut bufwro = BufWriter::new(tempfile::NamedTempFile::new_in(
        fpath.parent().expect("got invalid output file path"),
    )?);
    for line in config.iter() {
        writeln!(bufwro, "{}", line)?;
    }
    bufwro.into_inner()?.persist(fpath)?;
    Ok(())
}

fn main() {
    let matches = clap::App::new("update-dhcpcd-profile")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Erik Zscheile <erik.zscheile@gmail.com>")
        .about("replace the data of the given profile with the results of 'dhcpcd -U'")
        .arg(
            Arg::with_name("IFACE")
                .help("sets the interface to get the information from")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("PROFILE")
                .help("sets the profile to overwrite")
                .required(true)
                .index(2),
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .takes_value(true)
                .help("sets the output value"),
        )
        .get_matches();

    let iface = matches.value_of("IFACE").unwrap();
    let profile = matches.value_of("PROFILE").unwrap();
    let outfpath = matches.value_of("output").unwrap_or("/etc/dhcpcd.conf");
    let vars = get_dump(iface).expect("got invalid dump");

    let mut config =
        read_config_from_file("/etc/dhcpcd.conf").expect("unable to read /etc/dhcpcd.conf");

    // 1. remove profile
    {
        let mut is_in_this_profile = false;
        config.retain(|line| {
            if let Some(x) = line.strip_prefix("profile ") {
                is_in_this_profile = x == profile;
            }
            !is_in_this_profile
        });
    }

    // 2. add profile
    config.reserve(2 + vars.len());
    config.push(String::new());
    config.push("profile ".to_owned() + profile);
    config.extend(
        vars.iter()
            .map(|(key, value)| format!("static {}={}", key, value)),
    );
    config.shrink_to_fit();

    write_config_to_file(&config, outfpath).expect("unable to write output");
}
