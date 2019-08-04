use clap::Arg;
use std::collections::{HashMap, HashSet};
use std::io::{BufWriter, Write};
use std::process::Command;

fn get_dump(iface: &str) -> Result<HashMap<String, String>, String> {
    let dhdump = Command::new("dhcpcd")
        .args(&["-U", iface])
        .output()
        .expect("failed to spawn 'dhcpcd -U'");

    let mut vars = HashMap::new();

    for i in std::str::from_utf8(&dhdump.stdout)
        .expect("got non-utf8 result")
        .lines()
    {
        let (var, value) = match i.find(|x| x == '=') {
            Some(pos) => (&i[..pos], &i[pos + 1..]),
            None => continue,
        };
        if value.is_empty() {
            continue;
        }
        let value = if value.bytes().nth(0).unwrap() == b'\''
            && value.bytes().rev().nth(0).unwrap() == b'\''
        {
            &value[1..value.len() - 1]
        } else {
            value
        };
        vars.insert(var.to_owned(), value.to_owned());
    }

    if vars.is_empty() {
        return Err(std::str::from_utf8(&dhdump.stderr)
            .expect("got non-utf8 result")
            .to_owned());
    }

    let selected_vars: HashSet<&str> = vec![
        "broadcast_address",
        "domain_name_servers",
        "ip_address",
        "routers",
        "subnet_cidr",
    ]
    .into_iter()
    .collect();

    vars.retain(|var, _| selected_vars.contains(&var[..]));
    Ok(vars)
}

struct ConfigData(Vec<String>);

impl ConfigData {
    pub fn replace_profile(&mut self, profile: &str, vars: &HashMap<String, String>) {
        let profile_stline = "profile ".to_owned() + profile;

        // 1. remove profile
        {
            let mut is_in_this_profile = false;
            self.0.retain(|line| {
                if line.starts_with("profile ") {
                    is_in_this_profile = line == &profile_stline;
                }
                !is_in_this_profile
            });
        }

        // 2. add profile
        {
            self.0.reserve(2 + vars.len());
            self.0.push(String::new());
            self.0.push(profile_stline);
            for (key, value) in vars {
                let mut s = String::with_capacity(8 + key.len() + value.len());
                s += "static ";
                s += key;
                s += "=";
                s += value;
                self.0.push(s);
            }
        }

        self.0.shrink_to_fit();
    }

    pub fn read_from_file(fpath: &str) -> Result<ConfigData, failure::Error> {
        let fh_in = readfilez::read_from_file(std::fs::File::open(fpath))?;
        let mut ret = ConfigData(vec![]);
        for i in std::str::from_utf8(fh_in.as_slice())?.lines() {
            ret.0.push(i.to_owned());
        }
        Ok(ret)
    }

    pub fn write_to_file(&self, fpath: &str) -> Result<(), failure::Error> {
        let fpath = std::path::Path::new(fpath);
        let mut bufwro = BufWriter::new(tempfile::NamedTempFile::new_in(
            fpath.parent().expect("got invalid output file path"),
        )?);
        for line in &self.0 {
            writeln!(bufwro, "{}", line)?;
        }
        bufwro.into_inner()?.persist(fpath)?;
        Ok(())
    }
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
        ConfigData::read_from_file("/etc/dhcpcd.conf").expect("unable to read /etc/dhcpcd.conf");

    config.replace_profile(profile, &vars);

    config
        .write_to_file(outfpath)
        .expect("unable to write output");
}
