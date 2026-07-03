use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use super::classpath;
use super::gradle;
use super::java_ecosystem;
use super::languages::{self, merge_languages, push_unique};
use super::maven;
use super::native_build_tasks;

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectProfile {
    pub languages: Vec<String>,
    pub frameworks: Vec<String>,
    pub indexers: Vec<String>,
}

pub fn detect(ws: &Path) -> Result<ProjectProfile> {
    let mut languages = Vec::new();
    let mut frameworks = Vec::new();
    let mut indexers = Vec::new();

    detect_from_markers(ws, &mut languages, &mut frameworks, &mut indexers)?;
    merge_languages(&mut languages, &languages::scan_workspace_languages(ws)?);

    if indexers.is_empty()
        && languages
            .iter()
            .any(|l| l == "java" || l == "kotlin" || l == "groovy")
    {
        let has_java_index_roots = !gradle::find_all_gradle_roots(ws)?.is_empty()
            || !maven::find_all_maven_roots(ws)?.is_empty()
            || classpath::workspace_has_plain_java_sources(ws);
        if has_java_index_roots {
            push_unique(&mut indexers, "java");
        }
    }

    if !languages.is_empty() {
        push_unique(&mut indexers, "workspace-symbols");
    }

    Ok(ProjectProfile {
        languages,
        frameworks,
        indexers,
    })
}

pub fn indexing_label(profile: &ProjectProfile) -> String {
    languages::indexing_label(&profile.languages, &profile.frameworks)
}

fn detect_from_markers(
    ws: &Path,
    languages: &mut Vec<String>,
    frameworks: &mut Vec<String>,
    indexers: &mut Vec<String>,
) -> Result<()> {
    let has_gradle = ws.join("build.gradle").is_file() || ws.join("build.gradle.kts").is_file();
    let has_maven = ws.join("pom.xml").is_file();
    let has_gemfile = ws.join("Gemfile").is_file();
    let has_rakefile = ws.join("Rakefile").is_file() || ws.join("rakefile").is_file();
    let has_cargo = ws.join("Cargo.toml").is_file();
    let has_go = ws.join("go.mod").is_file();
    let has_python = ws.join("pyproject.toml").is_file()
        || ws.join("requirements.txt").is_file()
        || ws.join("setup.py").is_file()
        || ws.join("Pipfile").is_file();
    let has_node = ws.join("package.json").is_file();
    let has_composer = ws.join("composer.json").is_file();
    let has_swift = ws.join("Package.swift").is_file();
    let has_dart = ws.join("pubspec.yaml").is_file();
    let has_cmake = ws.join("CMakeLists.txt").is_file();
    let has_meson = ws.join("meson.build").is_file();
    let has_make = ws.join("Makefile").is_file()
        || ws.join("makefile").is_file()
        || ws.join("GNUmakefile").is_file();
    let has_vcpkg = ws.join("vcpkg.json").is_file();
    let has_conan = ws.join("conanfile.txt").is_file();
    let has_docker_compose = native_build_tasks::workspace_has_compose(ws);
    let has_dockerfile = ws.join("Dockerfile").is_file() || ws.join("dockerfile").is_file();
    let has_gemspec = has_extension_at_root(ws, "gemspec");
    let has_dotnet = ws.join("global.json").is_file() || has_extension_at_root(ws, "sln") || has_extension_at_root(ws, "csproj");

    if has_gradle || has_maven {
        push_unique(languages, "java");
        if ws.join("build.gradle.kts").is_file() {
            push_unique(languages, "kotlin");
        }
        push_unique(frameworks, if has_gradle { "gradle" } else { "maven" });
        if has_gradle {
            push_unique(languages, "groovy");
            if ws.join("grails-app").is_dir() {
                push_unique(frameworks, "grails");
            }
            for root in gradle::find_all_gradle_roots(ws)? {
                if gradle::is_spring_boot_project(&root) {
                    push_unique(frameworks, "spring-boot");
                    break;
                }
            }
        }
        push_unique(indexers, "java");
        let markers = java_ecosystem::scan_workspace_markers(ws)?;
        if markers.junit {
            push_unique(frameworks, "junit");
        }
        if markers.spring_test {
            push_unique(frameworks, "spring-test");
        }
        if markers.lombok {
            push_unique(frameworks, "lombok");
        }
        if markers.slf4j {
            push_unique(frameworks, "slf4j");
        }
    }

    if has_gemfile || has_gemspec || has_rakefile {
        push_unique(languages, "ruby");
        if has_gemfile || has_gemspec {
            push_unique(indexers, "ruby");
        }
        if ws.join("config/application.rb").is_file() || ws.join("config/routes.rb").is_file() {
            push_unique(frameworks, "rails");
            push_unique(indexers, "rails");
        } else if has_rakefile
            && (ws.join("config").is_dir()
                || ws.join("bin/rails").is_file()
                || ws.join("bin/rake").is_file())
        {
            push_unique(frameworks, "rails");
        }
    }
    if has_cargo {
        push_unique(languages, "rust");
        push_unique(indexers, "rust");
    }
    if has_go {
        push_unique(languages, "go");
        push_unique(indexers, "go");
    }
    if has_python {
        push_unique(languages, "python");
        if ws.join("manage.py").is_file() {
            push_unique(frameworks, "django");
        }
    }
    if has_node {
        push_unique(languages, "javascript");
        if ws.join("tsconfig.json").is_file() {
            push_unique(languages, "typescript");
        }
        if ws.join("next.config.js").is_file() || ws.join("next.config.mjs").is_file() {
            push_unique(frameworks, "nextjs");
        }
    }
    if has_composer {
        push_unique(languages, "php");
        if ws.join("artisan").is_file() {
            push_unique(frameworks, "laravel");
        }
    }
    if has_swift {
        push_unique(languages, "swift");
    }
    if has_dart {
        push_unique(languages, "dart");
        if ws.join("lib").join("main.dart").is_file() {
            push_unique(frameworks, "flutter");
        }
    }
    if has_cmake || has_meson || has_make || has_vcpkg || has_conan {
        push_unique(languages, "cpp");
        if has_cmake {
            push_unique(frameworks, "cmake");
        }
        if has_meson {
            push_unique(frameworks, "meson");
        }
        if has_make {
            push_unique(frameworks, "make");
        }
        if has_vcpkg {
            push_unique(frameworks, "vcpkg");
        }
        if has_conan {
            push_unique(frameworks, "conan");
        }
    }
    if has_docker_compose || has_dockerfile {
        push_unique(frameworks, "docker");
    }
    if has_dotnet {
        push_unique(languages, "csharp");
        push_unique(frameworks, "dotnet");
    }

    if classpath::workspace_has_plain_java_sources(ws) {
        push_unique(languages, "java");
        push_unique(indexers, "java");
    }

    Ok(())
}

fn has_extension_at_root(ws: &Path, ext: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(ws) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_plain_java_src() {
        let ws = std::env::temp_dir().join("reaper-profile-plain-java");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::write(ws.join("src/HelloWorld.java"), "public class HelloWorld {}\n").unwrap();
        let profile = detect(&ws).unwrap();
        assert!(profile.languages.contains(&"java".to_string()));
        assert!(profile.indexers.contains(&"java".to_string()));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn detects_spring_gradle() {
        let ws = std::env::temp_dir().join("reaper-profile-spring");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws.join("src/main/java")).unwrap();
        std::fs::write(
            &ws.join("build.gradle"),
            "plugins { id 'java'; id 'org.springframework.boot' version '3.2.0' }\n",
        )
        .unwrap();
        let profile = detect(&ws).unwrap();
        assert!(profile.languages.contains(&"java".to_string()));
        assert!(profile.frameworks.contains(&"spring-boot".to_string()));
        assert!(profile.indexers.contains(&"java".to_string()));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn detects_makefile_cpp_project() {
        let ws = std::env::temp_dir().join("reaper-profile-make");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("Makefile"), "all:\n\ttrue\n").unwrap();
        let profile = detect(&ws).unwrap();
        assert!(profile.languages.contains(&"cpp".to_string()));
        assert!(profile.frameworks.contains(&"make".to_string()));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn detects_gemspec_ruby() {
        let ws = std::env::temp_dir().join("reaper-profile-gemspec");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("mygem.gemspec"), "Gem::Specification.new {}\n").unwrap();
        let profile = detect(&ws).unwrap();
        assert!(profile.languages.contains(&"ruby".to_string()));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn detects_languages_from_source_files() {
        let ws = std::env::temp_dir().join("reaper-profile-mixed");
        let _ = std::fs::remove_dir_all(&ws);
        std::fs::create_dir_all(ws.join("cmd")).unwrap();
        std::fs::write(ws.join("cmd/main.go"), "package main\n").unwrap();
        std::fs::write(ws.join("script.lua"), "function main() end\n").unwrap();
        let profile = detect(&ws).unwrap();
        assert!(profile.languages.contains(&"go".to_string()));
        assert!(profile.languages.contains(&"lua".to_string()));
        assert!(profile.indexers.contains(&"workspace-symbols".to_string()));
        let _ = std::fs::remove_dir_all(&ws);
    }
}
