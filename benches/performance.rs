use std::collections::HashMap;

use apiforge::utils::TemplateEngine;
use apiforge::utils::{bump_version, format_version, parse_version, resolve_env_vars, BumpType};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_version_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("version_ops");

    group.bench_function("bump_patch", |b| {
        b.iter(|| bump_version(black_box("1.2.3"), black_box(BumpType::Patch)).unwrap())
    });

    group.bench_function("bump_minor", |b| {
        b.iter(|| bump_version(black_box("1.2.3"), black_box(BumpType::Minor)).unwrap())
    });

    let version = parse_version("1.2.3").unwrap();
    group.bench_function("format_version_tag", |b| {
        b.iter(|| format_version(black_box(&version), black_box("v{version}")))
    });

    group.finish();
}

fn bench_template_rendering(c: &mut Criterion) {
    let mut group = c.benchmark_group("template_rendering");
    let mut engine = TemplateEngine::new();
    let template = "release {{ project }} {{ version }} on {{ branch }}";
    let mut context = HashMap::new();
    context.insert("project".to_string(), "apiforge".to_string());
    context.insert("version".to_string(), "0.2.0".to_string());
    context.insert("branch".to_string(), "main".to_string());

    group.bench_function("render_release_template", |b| {
        b.iter(|| {
            engine
                .render(black_box(template), black_box(&context))
                .unwrap()
        })
    });

    group.finish();
}

fn bench_env_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("env_resolution");

    std::env::set_var("APIFORGE_BENCH_TOKEN", "bench-token");
    std::env::set_var("APIFORGE_BENCH_PROJECT", "apiforge");
    let input =
        "token=${APIFORGE_BENCH_TOKEN};project=${APIFORGE_BENCH_PROJECT};plain=value".to_string();

    group.bench_function("resolve_env_vars", |b| {
        b.iter(|| resolve_env_vars(black_box(&input)).unwrap())
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_version_operations,
    bench_template_rendering,
    bench_env_resolution
);
criterion_main!(benches);
