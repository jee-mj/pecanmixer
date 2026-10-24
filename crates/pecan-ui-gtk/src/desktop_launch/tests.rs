use super::*;
use gtk::glib;

#[test]
fn exact_legacy_normal_launch_and_hidden_autostart_shape() {
    let command = LaunchCommand::new("/synthetic/Pecan Mixer/pecanmixer").unwrap();
    assert_eq!(
        command.argv(LaunchMode::Visible),
        ["/synthetic/Pecan Mixer/pecanmixer"]
    );
    assert_eq!(
        command.argv(LaunchMode::Hidden),
        ["/synthetic/Pecan Mixer/pecanmixer", "--hidden"]
    );
    assert_eq!(
        command.exec_value(LaunchMode::Visible),
        "\"/synthetic/Pecan Mixer/pecanmixer\""
    );
    assert_eq!(command.autostart_entry(), legacy_entry(command.text()));
    assert!(command
        .autostart_entry()
        .contains("Exec=\"/synthetic/Pecan Mixer/pecanmixer\" --hidden\n"));
    assert_eq!(DESKTOP_FILENAME, "nl.jetse.pecanmixer.desktop");
}

#[test]
fn file_value_uses_two_escaping_layers() {
    let command = LaunchCommand::new("/synthetic/quo\"te/$cash/`tick\\slash/tool").unwrap();
    assert_eq!(
        command.exec_value(LaunchMode::Hidden),
        r#""/synthetic/quo\\"te/\\$cash/\\`tick\\\\slash/tool" --hidden"#
    );
    let keyfile = glib::KeyFile::new();
    keyfile
        .load_from_data(&command.autostart_entry(), glib::KeyFileFlags::NONE)
        .unwrap();
    let exec = keyfile.string("Desktop Entry", "Exec").unwrap();
    assert_eq!(
        exec.as_str(),
        r#""/synthetic/quo\"te/\$cash/\`tick\\slash/tool" --hidden"#
    );
    assert_eq!(
        keyfile.string("Desktop Entry", "Name").unwrap(),
        "Pecan Mixer"
    );
    assert!(!keyfile.boolean("Desktop Entry", "Terminal").unwrap());
    assert!(keyfile
        .boolean("Desktop Entry", "X-GNOME-Autostart-enabled")
        .unwrap());
}

#[test]
fn glib_value_and_argv_parsers_preserve_single_executable_for_reserved_paths() {
    // No program is launched. GLib decodes the desktop string and parses the
    // quoted argv, with no field-code requests or implicit shell evaluation.
    let samples = [
        "/synthetic/plain",
        "/synthetic/path with spaces/tool",
        "/synthetic/single'quote/\"double\"",
        "/synthetic/$dollar/`backtick`",
        "/synthetic/back\\slash",
        "/synthetic/<>&;~|*?#()/tool",
        "/synthetic/café/混合器",
    ];
    for path in samples {
        let command = LaunchCommand::new(path).unwrap();
        for mode in [LaunchMode::Visible, LaunchMode::Hidden] {
            let keyfile = glib::KeyFile::new();
            keyfile
                .load_from_data(
                    &format!("[Desktop Entry]\nExec={}\n", command.exec_value(mode)),
                    glib::KeyFileFlags::NONE,
                )
                .unwrap();
            let value = keyfile.string("Desktop Entry", "Exec").unwrap();
            let argv = glib::shell_parse_argv(value.as_str()).unwrap();
            let actual: Vec<String> = argv
                .iter()
                .map(|arg| arg.to_str().unwrap().to_owned())
                .collect();
            assert_eq!(actual, command.argv(mode), "{path:?}");
        }
    }
}

#[test]
fn appimage_precedence_and_relocation_are_injected_not_environment_discovery() {
    let mounted = Path::new("/synthetic/mount/usr/bin/pecanmixer");
    let old = Path::new("/synthetic/Downloads/Pecan Mixer.AppImage");
    let moved = Path::new("/synthetic/Applications/Pecan Mixer.AppImage");
    assert_eq!(
        LaunchCommand::resolve(mounted, None).unwrap().executable(),
        mounted
    );
    assert_eq!(
        LaunchCommand::resolve(mounted, Some(old))
            .unwrap()
            .executable(),
        old
    );
    assert_eq!(
        LaunchCommand::resolve(mounted, Some(moved))
            .unwrap()
            .executable(),
        moved
    );
    assert_eq!(
        LaunchCommand::resolve(mounted, Some(Path::new("relative.AppImage"))).unwrap_err(),
        CommandError::NotAbsolute
    );
}

#[test]
fn invalid_paths_never_become_lossy_or_inject_desktop_keys() {
    for (path, error) in [
        ("", CommandError::NotAbsolute),
        ("relative/tool", CommandError::NotAbsolute),
        ("/", CommandError::NoFileName),
        ("/synthetic/tool=bad", CommandError::EqualSign),
        ("/synthetic/tool%f", CommandError::PercentInExecutable),
        ("/synthetic/a\nHidden=true", CommandError::ControlCharacter),
        ("/synthetic/a\rtool", CommandError::ControlCharacter),
        ("/synthetic/a\ttool", CommandError::ControlCharacter),
        ("/synthetic/\0tool", CommandError::ControlCharacter),
    ] {
        assert_eq!(LaunchCommand::new(path).unwrap_err(), error);
    }
    assert_eq!(
        LaunchCommand::new(format!("/{}", "x".repeat(MAX_PATH_BYTES))).unwrap_err(),
        CommandError::TooLong
    );
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(b"/synthetic/\xff".to_vec()));
        assert_eq!(LaunchCommand::new(path).unwrap_err(), CommandError::NotUtf8);
    }
}

#[test]
fn exact_legacy_templates_are_recognized_without_accepting_custom_content() {
    for path in [
        "/synthetic/plain",
        "/synthetic/space path",
        "/synthetic/$cash",
        "/synthetic/\"quote\\slash`",
    ] {
        assert!(
            entry_format(legacy_entry(path).as_bytes()).is_some(),
            "{path}"
        );
        let command = LaunchCommand::new(path).unwrap();
        assert_eq!(
            entry_format(command.autostart_entry().as_bytes()),
            Some(EntryFormat::Native)
        );
    }
    let entry = LaunchCommand::new("/synthetic/tool")
        .unwrap()
        .autostart_entry();
    for custom in [
        entry.replace("--hidden", "--hidden --other"),
        entry.replace("Name=Pecan Mixer", "Name=Custom"),
        entry.replace(
            "X-GNOME-Autostart-enabled=true",
            "X-GNOME-Autostart-enabled=false",
        ),
        format!("{entry}Hidden=true\n"),
        entry.replace("\n", "\r\n"),
        entry.replace("/synthetic/tool", "relative"),
    ] {
        assert_eq!(entry_format(custom.as_bytes()), None, "{custom:?}");
    }
    // The old writer did not escape percent field codes. Do not treat those
    // historical nonworking paths as a supported managed launcher.
    assert_eq!(
        entry_format(
            entry
                .replace("/synthetic/tool", "/synthetic/tool%U")
                .as_bytes()
        ),
        None
    );
    assert_eq!(entry_format(&[0xff, 0x00]), None);
}
