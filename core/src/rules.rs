//! Which folders inside a project its own tools can rebuild.
//!
//! A project type is known by a marker file at its root (`package.json`, `Cargo.toml`…); the
//! folders listed for it are what that type's tools regenerate. The marker idea and the shape
//! of this table come from kondo by Trevor Billington (MIT, github.com/tbillington/kondo);
//! the table itself was written fresh for Headroom.

#[derive(Debug)]
pub struct Rule {
    pub name: &'static str,
    /// Exact file names, or `*.ext` to match by suffix.
    pub markers: &'static [&'static str],
    pub dirs: &'static [&'static str],
    /// The command that brings the folders back; empty when the tool refills them on its own.
    pub rebuild: &'static [&'static [&'static str]],
}

const NONE: &[&[&str]] = &[];

pub const RULES: &[Rule] = &[
    // node_modules and the build folders get their commands from package.json (see rebuild.rs).
    Rule {
        name: "Node",
        markers: &["package.json"],
        dirs: &[
            "node_modules",
            ".next",
            ".nuxt",
            ".output",
            ".svelte-kit",
            ".angular",
            ".docusaurus",
            ".turbo",
            ".parcel-cache",
            ".vite",
            ".expo",
            "dist",
            "build",
            "out",
        ],
        rebuild: NONE,
    },
    Rule { name: "Rust", markers: &["Cargo.toml"], dirs: &["target"], rebuild: NONE },
    Rule {
        name: "Python",
        markers: &["pyproject.toml", "setup.py", "setup.cfg", "requirements.txt", "Pipfile"],
        dirs: &[".pytest_cache", ".mypy_cache", ".ruff_cache", ".tox", ".nox"],
        rebuild: NONE,
    },
    Rule { name: "SwiftPM", markers: &["Package.swift"], dirs: &[".build"], rebuild: &[&["swift", "build"]] },
    Rule { name: "Xcode", markers: &["*.xcodeproj", "*.xcworkspace"], dirs: &["DerivedData", "build"], rebuild: NONE },
    Rule { name: "CocoaPods", markers: &["Podfile"], dirs: &["Pods"], rebuild: &[&["pod", "install"]] },
    Rule {
        name: "Gradle",
        markers: &["build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"],
        dirs: &[".gradle", "build"],
        rebuild: &[&["gradle", "assemble"]],
    },
    Rule {
        name: "Maven",
        markers: &["pom.xml"],
        dirs: &["target"],
        rebuild: &[&["mvn", "-q", "-DskipTests", "package"]],
    },
    Rule { name: "sbt", markers: &["build.sbt"], dirs: &["target", "project/target"], rebuild: &[&["sbt", "compile"]] },
    Rule {
        name: "CMake",
        markers: &["CMakeLists.txt"],
        dirs: &["build"],
        rebuild: &[&["cmake", "-S", ".", "-B", "build"], &["cmake", "--build", "build"]],
    },
    Rule {
        name: "Flutter",
        markers: &["pubspec.yaml"],
        dirs: &[".dart_tool", "build"],
        rebuild: &[&["flutter", "pub", "get"]],
    },
    Rule { name: "Stack", markers: &["stack.yaml"], dirs: &[".stack-work"], rebuild: &[&["stack", "build"]] },
    Rule {
        name: "Cabal",
        markers: &["cabal.project", "*.cabal"],
        dirs: &["dist-newstyle"],
        rebuild: &[&["cabal", "build"]],
    },
    Rule {
        name: "Elixir",
        markers: &["mix.exs"],
        dirs: &["_build", "deps"],
        rebuild: &[&["mix", "deps.get"], &["mix", "compile"]],
    },
    Rule {
        name: "Zig",
        markers: &["build.zig"],
        dirs: &["zig-cache", ".zig-cache", "zig-out"],
        rebuild: &[&["zig", "build"]],
    },
    Rule { name: "Godot", markers: &["project.godot"], dirs: &[".godot"], rebuild: NONE },
    Rule {
        name: ".NET",
        markers: &["*.csproj", "*.fsproj", "*.sln"],
        dirs: &["bin", "obj"],
        rebuild: &[&["dotnet", "build"]],
    },
    Rule { name: "Unity", markers: &["Assembly-CSharp.csproj"], dirs: &["Library", "Temp", "Obj"], rebuild: NONE },
    Rule {
        name: "Unreal",
        markers: &["*.uproject"],
        dirs: &["Binaries", "Intermediate", "DerivedDataCache"],
        rebuild: NONE,
    },
    Rule { name: "Composer", markers: &["composer.json"], dirs: &["vendor"], rebuild: &[&["composer", "install"]] },
    Rule { name: "Bundler", markers: &["Gemfile.lock"], dirs: &["vendor/bundle"], rebuild: &[&["bundle", "install"]] },
    Rule {
        name: "Terraform",
        markers: &[".terraform.lock.hcl"],
        dirs: &[".terraform"],
        rebuild: &[&["terraform", "init"]],
    },
    Rule { name: "Pixi", markers: &["pixi.toml"], dirs: &[".pixi"], rebuild: &[&["pixi", "install"]] },
    Rule { name: "PlatformIO", markers: &["platformio.ini"], dirs: &[".pio"], rebuild: &[&["pio", "run"]] },
    Rule { name: "Turborepo", markers: &["turbo.json"], dirs: &[".turbo"], rebuild: NONE },
];

/// Names that are often build output but just as often the work itself (a hand-made site in
/// `build/`, vendored code): they only count when git says they are ignored.
pub const MAYBE: &[&str] =
    &["build", "dist", "out", "bin", "obj", "vendor", "Library", "Temp", "Obj", "Binaries", "zig-out"];

/// A virtualenv may only go if one of these sits beside it or at the project root.
pub const PY_MANIFESTS: &[&str] = &[
    "requirements.txt",
    "requirements-dev.txt",
    "requirements*.txt",
    "pyproject.toml",
    "Pipfile",
    "Pipfile.lock",
    "poetry.lock",
    "uv.lock",
    "setup.py",
    "setup.cfg",
    "environment.yml",
];

/// Never looked into: version control, dependency trees (every package in them looks like a
/// project of its own), bundles, the Trash.
pub fn never_enter(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".hg" | ".svn" | ".Trash" | ".Trashes" | "node_modules" | "site-packages" | ".DocumentRevisions-V100"
    ) || [".app", ".framework", ".photoslibrary", ".xcarchive", ".bundle", ".musiclibrary", ".tvlibrary"]
        .iter()
        .any(|e| name.ends_with(e))
}

pub fn matches(names: &[String], marker: &str) -> bool {
    match marker.find('*') {
        Some(i) => {
            let (pre, post) = (&marker[..i], &marker[i + 1..]);
            names.iter().any(|n| n.len() > pre.len() + post.len() && n.starts_with(pre) && n.ends_with(post))
        }
        None => names.iter().any(|n| n == marker),
    }
}

pub fn rules_for(names: &[String]) -> impl Iterator<Item = &'static Rule> + '_ {
    RULES.iter().filter(move |r| r.markers.iter().any(|m| matches(names, m)))
}

pub fn is_maybe(dir: &str) -> bool {
    MAYBE.contains(&dir.rsplit('/').next().unwrap_or(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn markers_match_exactly_or_by_suffix() {
        assert!(matches(&n(&["App.xcodeproj"]), "*.xcodeproj"));
        assert!(!matches(&n(&[".xcodeproj"]), "*.xcodeproj"));
        assert!(matches(&n(&["requirements-test.txt"]), "requirements*.txt"));
        assert!(!matches(&n(&["package.json.bak"]), "package.json"));
        let names: Vec<_> = rules_for(&n(&["package.json", "Cargo.toml"])).map(|r| r.name).collect();
        assert_eq!(names, ["Node", "Rust"]);
    }

    #[test]
    fn about_twenty_five_project_types() {
        assert!(RULES.len() >= 25, "{}", RULES.len());
        assert!(is_maybe("build") && is_maybe("project/out") && !is_maybe("node_modules"));
        assert!(never_enter("node_modules") && never_enter("Foo.app") && never_enter(".git"));
        assert!(!never_enter("src"));
    }
}
