use super::*;
use gtk::glib;

#[test]
fn native_identity_visible_launch_and_explicit_hidden_tray_shape() {
    let command = LaunchCommand::new("/synthetic/Pecan Mixer/pecanmixer").unwrap();
    assert_eq!(
        command.argv(LaunchMode::Visible),
        ["/synthetic/Pecan Mixer/pecanmixer"]
    );
    assert_eq!(
        command.argv(LaunchMode::Hidden),
        ["/synthetic/Pecan Mixer/pecanmixer", "--hidden", "--tray"]
    );
    assert_eq!(
        command.exec_value(LaunchMode::Visible),
        "\"/synthetic/Pecan Mixer/pecanmixer\""
    );
    assert_ne!(command.autostart_entry(), legacy_entry(command.text()));
    assert!(command
        .autostart_entry()
        .contains("Exec=\"/synthetic/Pecan Mixer/pecanmixer\" --hidden --tray\n"));
    assert_eq!(DESKTOP_FILENAME, "nl.jetse.pecanmixer.Native.desktop");
    assert_ne!(DESKTOP_FILENAME, LEGACY_DESKTOP_FILENAME);
    assert!(command
        .autostart_entry()
        .contains("X-PecanMixer-Native-Autostart=1\n"));
}

#[test]
fn file_value_uses_two_escaping_layers() {
    let command = LaunchCommand::new("/synthetic/quo\"te/$cash/`tick\\slash/tool").unwrap();
    assert_eq!(
        command.exec_value(LaunchMode::Hidden),
        r#""/synthetic/quo\\"te/\\$cash/\\`tick\\\\slash/tool" --hidden --tray"#
    );
    let keyfile = glib::KeyFile::new();
    keyfile
        .load_from_data(&command.autostart_entry(), glib::KeyFileFlags::NONE)
        .unwrap();
    let exec = keyfile.string("Desktop Entry", "Exec").unwrap();
    assert_eq!(
        exec.as_str(),
        r#""/synthetic/quo\"te/\$cash/\`tick\\slash/tool" --hidden --tray"#
    );
    assert_eq!(
        keyfile.string("Desktop Entry", "Name").unwrap(),
        "Pecan Mixer (Native)"
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

#[test]
fn explicit_paths_roundtrip_through_actual_native_option_parser_in_both_launch_modes() {
    for settings in [
        None,
        Some(PathBuf::from("/synthetic/preferences/native.json")),
    ] {
        for root in [None, Some(PathBuf::from("/synthetic/recordings"))] {
            let command = LaunchCommand::new("/synthetic/Pecan Native")
                .unwrap()
                .with_paths(settings.clone(), root.clone())
                .unwrap();
            assert_eq!(command.settings(), settings.as_deref());
            assert_eq!(command.recording_root(), root.as_deref());
            for mode in [LaunchMode::Visible, LaunchMode::Hidden] {
                let options = crate::startup::Options::parse(
                    command.argv(mode).into_iter().skip(1).map(Into::into),
                )
                .unwrap()
                .unwrap();
                assert_eq!(options.settings, settings);
                assert_eq!(options.recording_root, root);
                assert_eq!(options.hidden, mode == LaunchMode::Hidden);
                assert_eq!(options.tray, mode == LaunchMode::Hidden);
            }
        }
    }
}

#[test]
fn explicit_path_arguments_have_two_escape_layers_and_literal_percent_codes() {
    for parent in [
        "/synthetic/space path",
        "/synthetic/\"quote' $cash `tick` back\\slash",
        "/synthetic/$(inert);<>&|*?()",
        "/synthetic/equals=valid",
        "/synthetic/literal%U-%%-name",
        "/synthetic/café/混合器",
    ] {
        let command = LaunchCommand::new("/synthetic/Pecan Native")
            .unwrap()
            .with_paths(
                Some(PathBuf::from(format!("{parent}/settings.json"))),
                Some(PathBuf::from(format!("{parent}/captures"))),
            )
            .unwrap();
        for mode in [LaunchMode::Visible, LaunchMode::Hidden] {
            let keyfile = glib::KeyFile::new();
            keyfile
                .load_from_data(
                    &format!("[Desktop Entry]\nExec={}\n", command.exec_value(mode)),
                    glib::KeyFileFlags::NONE,
                )
                .unwrap();
            let decoded = keyfile.string("Desktop Entry", "Exec").unwrap();
            let parsed = glib::shell_parse_argv(decoded.as_str()).unwrap();
            let parsed = parsed
                .iter()
                .map(|s| s.to_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            // This parser does not launch or expand desktop field codes. A
            // literal % remains doubled until the launcher's single expansion.
            let expected = command
                .argv(mode)
                .into_iter()
                .map(|s| s.replace('%', "%%"))
                .collect::<Vec<_>>();
            assert_eq!(parsed, expected);
        }
        assert_eq!(
            entry_format(command.autostart_entry().as_bytes()),
            Some(EntryFormat::Native)
        );
        assert_eq!(
            parse_native(&command.exec_value(LaunchMode::Hidden)),
            Some(command)
        );
    }
}

#[test]
fn invalid_argument_paths_and_combined_escaped_size_never_generate_unloadable_entries() {
    for (path, error) in [
        ("relative", CommandError::NotAbsolute),
        (
            "/synthetic/line\nExec=injected",
            CommandError::ControlCharacter,
        ),
        ("/synthetic/tab\tpath", CommandError::ControlCharacter),
        ("/synthetic/nul\0path", CommandError::ControlCharacter),
    ] {
        for settings in [true, false] {
            let paths = if settings {
                (Some(PathBuf::from(path)), None)
            } else {
                (None, Some(PathBuf::from(path)))
            };
            assert_eq!(
                LaunchCommand::new("/synthetic/app")
                    .unwrap()
                    .with_paths(paths.0, paths.1)
                    .unwrap_err(),
                error
            );
        }
    }
    assert_eq!(
        LaunchCommand::new("/synthetic/app")
            .unwrap()
            .with_paths(Some("/".into()), None)
            .unwrap_err(),
        CommandError::NoFileName
    );
    assert!(LaunchCommand::new("/synthetic/app")
        .unwrap()
        .with_paths(None, Some("/".into()))
        .is_ok());
    let long = PathBuf::from(format!("/{}", "x".repeat(MAX_PATH_BYTES)));
    assert_eq!(
        LaunchCommand::new("/synthetic/app")
            .unwrap()
            .with_paths(Some(long), None)
            .unwrap_err(),
        CommandError::TooLong
    );
    let worst = PathBuf::from(format!("/{}", "\\".repeat(MAX_PATH_BYTES - 1)));
    assert_eq!(
        LaunchCommand::new(worst.clone())
            .unwrap()
            .with_paths(Some(worst.clone()), Some(worst))
            .unwrap_err(),
        CommandError::TooLong
    );
    let ordinary = PathBuf::from(format!("/{}", "x".repeat(MAX_PATH_BYTES - 1)));
    let command = LaunchCommand::new(ordinary.clone())
        .unwrap()
        .with_paths(Some(ordinary.clone()), Some(ordinary))
        .unwrap();
    assert!(command.autostart_entry().len() <= storage::MAX_ENTRY_BYTES as usize);
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let bad = PathBuf::from(std::ffi::OsString::from_vec(b"/synthetic/\xff".to_vec()));
        assert_eq!(
            LaunchCommand::new("/synthetic/app")
                .unwrap()
                .with_paths(Some(bad.clone()), None)
                .unwrap_err(),
            CommandError::NotUtf8
        );
        assert_eq!(
            LaunchCommand::new("/synthetic/app")
                .unwrap()
                .with_paths(None, Some(bad))
                .unwrap_err(),
            CommandError::NotUtf8
        );
    }
}

#[test]
fn only_exact_native_argument_template_is_recognized() {
    let command = LaunchCommand::new("/synthetic/app")
        .unwrap()
        .with_paths(
            Some("/synthetic/literal%U/settings.json".into()),
            Some("/synthetic/recordings".into()),
        )
        .unwrap();
    let entry = command.autostart_entry();
    assert_eq!(entry_format(entry.as_bytes()), Some(EntryFormat::Native));
    for altered in [
        entry.replace("--settings", "--unknown"),
        entry.replace("--tray", ""),
        entry.replace("%%U", "%U"),
        entry.replace("X-PecanMixer-Native-Autostart=1\n", ""),
        entry.replace("--settings", "--recording-root"),
    ] {
        assert_eq!(entry_format(altered.as_bytes()), None);
    }
}

#[test]
fn management_context_roundtrips_existing_cli_without_guessing_authority() {
    for root in [
        "/synthetic/config root",
        "/synthetic/\"$cash`tick\\literal%U/café",
    ] {
        let executable = PathBuf::from("/synthetic/native \"$cash`app\\image");
        let command = LaunchCommand::new(executable.clone())
            .unwrap()
            .with_paths(
                Some("/synthetic/settings %U.json".into()),
                Some("/synthetic/recordings".into()),
            )
            .unwrap()
            .with_management_context(root.into())
            .unwrap();
        for mode in [LaunchMode::Visible, LaunchMode::Hidden] {
            let options = crate::startup::Options::parse(
                command.argv(mode).into_iter().skip(1).map(Into::into),
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                options.autostart_config_root.as_deref(),
                Some(Path::new(root))
            );
            assert_eq!(options.native_executable.as_ref(), Some(&executable));
            assert_eq!(options.settings.as_deref(), command.settings());
            assert_eq!(options.recording_root.as_deref(), command.recording_root());
            assert_eq!(options.hidden, mode == LaunchMode::Hidden);
            assert_eq!(options.tray, mode == LaunchMode::Hidden);
            let file = glib::KeyFile::new();
            file.load_from_data(
                &format!("[Desktop Entry]\nExec={}\n", command.exec_value(mode)),
                glib::KeyFileFlags::NONE,
            )
            .unwrap();
            let raw = file.string("Desktop Entry", "Exec").unwrap();
            let argv = glib::shell_parse_argv(raw.as_str()).unwrap();
            let actual = argv
                .iter()
                .map(|s| s.to_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            let expected = command
                .argv(mode)
                .into_iter()
                .map(|s| s.replace('%', "%%"))
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
        assert_eq!(
            native_command(command.autostart_entry().as_bytes()),
            Some(command)
        );
    }
}

#[test]
fn management_context_is_explicit_paired_bounded_and_cannot_rebind_executable() {
    let plain = LaunchCommand::new("/synthetic/native").unwrap();
    assert!(!plain
        .argv(LaunchMode::Hidden)
        .iter()
        .any(|v| v == "--native-executable" || v == "--autostart-config-root"));
    let managed = plain
        .clone()
        .with_management_context("/synthetic/config".into())
        .unwrap();
    assert_eq!(
        managed
            .clone()
            .with_management_context("/synthetic/other".into())
            .unwrap_err(),
        CommandError::ManagementContextMismatch
    );
    let entry = managed.autostart_entry();
    for altered in [
        entry.replace(
            "--native-executable \"/synthetic/native\"",
            "--native-executable \"/synthetic/foreign\"",
        ),
        entry.replace("--native-executable \"/synthetic/native\"", ""),
        entry.replace("--autostart-config-root \"/synthetic/config\"", ""),
        entry.replace("--autostart-config-root", "--unknown-root"),
    ] {
        assert_eq!(entry_format(altered.as_bytes()), None);
    }
    assert_eq!(
        plain
            .clone()
            .with_management_context("relative".into())
            .unwrap_err(),
        CommandError::NotAbsolute
    );
    assert_eq!(
        plain
            .clone()
            .with_management_context("/synthetic/../config".into())
            .unwrap_err(),
        CommandError::InvalidManagementRoot
    );
    assert_eq!(
        plain
            .with_management_context("/synthetic/line\npath".into())
            .unwrap_err(),
        CommandError::ControlCharacter
    );
    let long_executable =
        LaunchCommand::new(format!("/{}", "\\".repeat(MAX_PATH_BYTES - 1))).unwrap();
    assert_eq!(
        long_executable
            .with_management_context("/synthetic/config".into())
            .unwrap_err(),
        CommandError::TooLong
    );
}
