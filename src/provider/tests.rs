use super::*;

fn github_project(name: &str, archived: bool, fork: bool) -> github::GithubProject {
    serde_json::from_value(serde_json::json!({
        "name": name.rsplit('/').next().expect("name is not empty"),
        "full_name": name,
        "clone_url": "https://example.com/repo.git",
        "ssh_url": "git@example.com:repo.git",
        "private": false,
        "fork": fork,
        "archived": archived,
    }))
    .expect("valid GitHub project fixture")
}

fn config_filter(toml: &str) -> Filter {
    let config: config::ConfigProviderFilter = toml::from_str(toml).expect("valid filter fixture");
    config.into()
}

#[test]
fn old_config_keeps_archived_repositories_and_forks() {
    let filter = config_filter("access = true");
    assert!(!filter.empty(), "access selects repositories");
    for archived in [false, true] {
        for fork in [false, true] {
            assert!(
                filter.includes(&github_project("org/repo", archived, fork)),
                "omitted exclusion filters preserve existing behavior"
            );
        }
    }
}

#[test]
fn archive_filter_is_opt_in() {
    for ignore_archived in [false, true] {
        let filter = config_filter(&format!(
            "access = true\nignore_archived = {ignore_archived}"
        ));
        for archived in [false, true] {
            assert_eq!(
                filter.includes(&github_project("org/repo", archived, false)),
                !(ignore_archived && archived),
            );
        }
    }
}

#[test]
fn exclusions_without_wildcards_match_only_exact_full_paths() {
    let filter = config_filter("access = true\nexclude = ['org/repo', 'group/sub/repo']");
    for name in ["org/repo", "group/sub/repo"] {
        assert!(
            !filter.includes(&github_project(name, false, false)),
            "exact full paths are excluded"
        );
    }
    for name in ["other/repo", "org/repo-more", "org/Repo", "group/repo"] {
        assert!(
            filter.includes(&github_project(name, false, false)),
            "similar names and other namespaces must stay included"
        );
    }
    for entry in ["repo", "^org/repo$", "https://example.com/org/repo.git"] {
        let filter = config_filter(&format!("access = true\nexclude = ['{entry}']"));
        assert!(
            filter.includes(&github_project("org/repo", false, false)),
            "exclusions are not bare names, URLs, or regexes"
        );
    }
}

#[test]
fn wildcard_exclusions_match_complete_paths() {
    for (pattern, path, expected) in [
        ("zscaler-config/*", "zscaler-config/repo", true),
        ("zscaler-config/*", "zscaler-config/sub/repo", true),
        ("zscaler-config/*", "other/zscaler-config/repo", false),
        ("zscaler-config/*", "zscaler-config-extra/repo", false),
        ("zscaler-config/*", "Zscaler-config/repo", false),
        ("team/legacy-*", "team/legacy-service", true),
        ("team/legacy-*", "team/new-legacy-service", false),
        ("*/repo", "team/repo", true),
        ("*/repo", "team/sub/repo", true),
        ("*/repo", "team/repo-extra", false),
        ("team/*-config", "team/repo-config", true),
        ("team/*-config", "team/repo-config-old", false),
        ("te*/sub/*-config", "team/sub/repo-config", true),
        ("te*/sub/*-config", "team/other/repo-config", false),
        ("org/re*po", "org/repo", true),
        ("org/re*po", "org/repository", false),
        ("org/repo*repo", "org/repo", false),
        ("org/*ab*ab", "org/ab", false),
        ("org/*ab*ab", "org/abab", true),
        ("org/**repo**", "org/repo", true),
        ("org/**repo**", "org/sub/repo-extra", true),
        ("*", "org/repo", true),
        ("**", "org/repo", true),
        ("*", "", true),
        ("", "", true),
        ("", "org/repo", false),
        ("org/repo", "org/repo", true),
        ("org/repo", "org/repo-extra", false),
        ("org/*.config", "org/repo.config", true),
        ("org/*.config", "org/repo-config", false),
        ("org/*?", "org/repo?", true),
        ("org/*?", "org/repo", false),
        ("org/*[ab]", "org/repo[ab]", true),
        ("org/*[ab]", "org/repoa", false),
        ("org/*{a,b}", "org/repo{a,b}", true),
        ("org/*{a,b}", "org/repoa", false),
        ("org/ø*é", "org/øvelse-é", true),
        ("org/ø*é", "org/øvelse-e", false),
    ] {
        assert_eq!(
            matches_exclusion(pattern, path),
            expected,
            "{pattern}: {path}"
        );
    }
}

#[test]
fn wildcard_matching_agrees_with_anchored_regex() {
    fn words(alphabet: &[char], max_len: usize) -> Vec<String> {
        let mut words = vec![String::new()];
        let mut level = vec![String::new()];
        for _ in 0..max_len {
            level = level
                .iter()
                .flat_map(|prefix| {
                    alphabet
                        .iter()
                        .map(move |letter| format!("{prefix}{letter}"))
                })
                .collect();
            words.extend(level.iter().cloned());
        }
        words
    }

    let paths = words(&['a', 'b', '/'], 5);
    for pattern in words(&['a', 'b', '/', '*'], 4) {
        let expression = regex::escape(&pattern).replace(r"\*", ".*");
        let reference = regex::Regex::new(&format!(r"\A{expression}\z"))
            .expect("escaped wildcard pattern is valid regex");
        for path in &paths {
            assert_eq!(
                matches_exclusion(&pattern, path),
                reference.is_match(path),
                "pattern {pattern:?}, path {path:?}",
            );
        }
    }
}

#[test]
fn wildcard_and_exact_exclusions_work_together() {
    let filter = config_filter(
        "access = true\nexclude = ['zscaler-config/*', 'other/one-repo', 'team/legacy-*']",
    );
    for path in [
        "zscaler-config/repo",
        "zscaler-config/sub/repo",
        "other/one-repo",
        "team/legacy-service",
    ] {
        assert!(
            !filter.includes(&github_project(path, false, false)),
            "{path}"
        );
    }
    for path in [
        "other/another-repo",
        "team/new-service",
        "zscaler-config-extra/repo",
    ] {
        assert!(
            filter.includes(&github_project(path, false, false)),
            "{path}"
        );
    }
}

#[test]
fn wildcard_exclusions_do_not_select_repositories() {
    assert!(
        config_filter("exclude = ['*']").empty(),
        "wildcards do not select repositories"
    );
    let filter = config_filter("access = true\nexclude = ['*']");
    assert!(!filter.empty(), "access still selects repositories");
    assert!(
        !filter.includes(&github_project("org/repo", false, false)),
        "star excludes everything"
    );
}

#[test]
fn setting_exclusions_replaces_previous_patterns() {
    let filter = config_filter("access = true\nexclude = ['old/*', 'other/repo']")
        .with_exclusions(false, vec!["new/*".to_owned()]);
    assert!(
        filter.includes(&github_project("old/repo", false, false)),
        "old wildcard was replaced"
    );
    assert!(
        filter.includes(&github_project("other/repo", false, false)),
        "old exact path was replaced"
    );
    assert!(
        !filter.includes(&github_project("new/repo", false, false)),
        "new wildcard is active"
    );
}

#[test]
fn exclusion_filters_combine_with_fork_filter() {
    let filter =
        config_filter("access = true\nfork = false\nignore_archived = true\nexclude = ['org/sk*']");
    for name in ["org/keep", "org/skip"] {
        for archived in [false, true] {
            for fork in [false, true] {
                assert_eq!(
                    filter.includes(&github_project(name, archived, fork)),
                    name == "org/keep" && !archived && !fork,
                );
            }
        }
    }
}

#[test]
fn exclusions_do_not_select_repositories() {
    let filter = config_filter("ignore_archived = true\nexclude = ['org/repo']");
    assert!(
        filter.empty(),
        "exclusions alone must not select repositories"
    );
    assert!(
        Filter::from(config::ConfigProviderFilter::default()).empty(),
        "missing filters must not select repositories"
    );
}

#[test]
fn config_conversion_preserves_all_inclusion_filters() {
    let filter =
        config_filter("access = true\nowner = true\nusers = ['alice']\ngroups = ['team/subgroup']");
    assert!(filter.owner, "owner selection is preserved");
    assert!(filter.access, "access selection is preserved");
    assert_eq!(filter.users.len(), 1);
    assert_eq!(filter.users.first().expect("one user").to_string(), "alice");
    assert_eq!(filter.groups.len(), 1);
    assert_eq!(
        filter.groups.first().expect("one group").to_string(),
        "team/subgroup"
    );
    for selection in ["owner = true", "users = ['alice']", "groups = ['team']"] {
        assert!(!config_filter(selection).empty(), "selection is not empty");
    }
}

#[test]
fn gitlab_archive_metadata_and_nested_namespace() {
    for archived in [false, true] {
        let mut data = serde_json::json!({
            "path": "repo",
            "path_with_namespace": "group/sub/repo",
            "http_url_to_repo": "https://example.com/group/sub/repo.git",
            "ssh_url_to_repo": "git@example.com:group/sub/repo.git",
            "visibility": "private",
            "archived": archived,
        });
        let project: gitlab::GitlabProject =
            serde_json::from_value(data.clone()).expect("valid GitLab project");
        assert_eq!(project.is_archived(), archived);
        assert_eq!(
            config_filter("access = true\nignore_archived = true").includes(&project),
            !archived,
        );
        assert!(
            !config_filter("access = true\nexclude = ['group/sub/repo']").includes(&project),
            "nested GitLab paths are excluded"
        );
        data.as_object_mut().expect("object").remove("archived");
        let project: gitlab::GitlabProject =
            serde_json::from_value(data).expect("archive metadata is optional");
        assert!(!project.is_archived(), "missing metadata defaults to false");
    }
}

#[test]
fn github_missing_archive_metadata_is_backward_compatible() {
    let project: github::GithubProject = serde_json::from_value(serde_json::json!({
        "name": "repo",
        "full_name": "org/repo",
        "clone_url": "https://example.com/org/repo.git",
        "ssh_url": "git@example.com:org/repo.git",
        "private": false,
        "fork": false,
    }))
    .expect("archive metadata is optional");
    assert!(!project.is_archived(), "missing metadata defaults to false");
}

#[test]
fn toml_and_yaml_provider_configs_accept_new_filters() {
    let config: config::ConfigProvider = toml::from_str(
        "provider = 'github'\ntoken_command = 'echo test'\nroot = '.'\n\
         [filters]\naccess = true\nignore_archived = true\nexclude = ['org/repo']",
    )
    .expect("valid provider config");
    let yaml = serde_yaml::to_string(&config).expect("serializes to YAML");
    let roundtrip: config::ConfigProvider =
        serde_yaml::from_str(&yaml).expect("valid YAML provider config");
    let filter = Filter::from(roundtrip.filters.expect("filters are present"));
    assert!(
        filter.ignore_archived,
        "archive setting survives round trip"
    );
    assert!(
        filter.exclude.contains("org/repo"),
        "exclusion survives round trip"
    );
}

#[test]
fn invalid_filter_values_are_rejected() {
    for toml in [
        "ignore_archived = 'true'",
        "exclude = 'org/repo'",
        "exclude = [42]",
        "ignore_archive = true",
    ] {
        toml::from_str::<config::ConfigProviderFilter>(toml)
            .expect_err("invalid filter configuration must be rejected");
    }
}
