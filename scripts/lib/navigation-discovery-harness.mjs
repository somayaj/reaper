/**
 * Java navigation discovery — JDK, Spring/libraries, Gradle + Maven multi-module.
 */

function extractRustFnBody(src, name) {
  const block = src.match(new RegExp(`fn ${name}[\\s\\S]*?(?=\\nfn )`))?.[0]
    || src.match(new RegExp(`pub fn ${name}[\\s\\S]*?(?=\\npub fn )`))?.[0]
    || '';
  const brace = block.indexOf('{');
  if (brace === -1) return '';
  let depth = 1;
  for (let i = brace + 1; i < block.length; i += 1) {
    const ch = block[i];
    if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) return block.slice(brace + 1, i);
    }
  }
  return '';
}

export function testNavigationDiscoveryRegression(
  classpathRs,
  modRs,
  projectJobsRs,
  ok,
) {
  const resolveTypeBody = extractRustFnBody(classpathRs, 'resolve_type_by_fqcn');
  const resolveLibBody = extractRustFnBody(classpathRs, 'resolve_library_type_location');
  const resolveJdkFqcnBody = extractRustFnBody(classpathRs, 'resolve_jdk_fqcn_location');
  const resolveFqcnJdkBody = extractRustFnBody(classpathRs, 'resolve_fqcn_from_jdk_files');
  const jdkSearchDirsBody = extractRustFnBody(classpathRs, 'jdk_source_search_dirs');
  const jdkScopeBody = extractRustFnBody(classpathRs, 'jdk_source_scope');

  ok(
    classpathRs.includes('fn jdk_source_scope')
      && jdkScopeBody.includes('maven::is_maven_project_root')
      && jdkScopeBody.includes('find_maven_reactor_root')
      && jdkScopeBody.includes('gradle_index_source_scope'),
    'discovery: JDK scope uses Maven reactor root or Gradle settings root',
  );

  ok(
    classpathRs.includes('pub fn warm_jdk_sources')
      && classpathRs.includes('jdk_source_scope(&root)'),
    'discovery: warm_jdk_sources extracts JDK per Maven/Gradle repo scope',
  );

  ok(
    projectJobsRs.includes('warm_jdk_sources')
      && projectJobsRs.includes('is_java_indexable_workspace'),
    'discovery: workspace open kicks off JDK warm for Java projects',
  );

  ok(
    classpathRs.includes('find_all_maven_roots')
      && classpathRs.includes('find_all_gradle_roots'),
    'discovery: index roots include both Maven and Gradle modules',
  );

  ok(
    classpathRs.includes('fn is_jdk_fqcn')
      && resolveTypeBody.includes('is_jdk_fqcn(fqcn)')
      && resolveTypeBody.includes('resolve_jdk_fqcn_location')
      && resolveLibBody.includes('if is_jdk_fqcn(fqcn)'),
    'discovery: JDK FQCNs use dedicated JDK path and skip slow library/Maven extract on F12',
  );

  ok(
    resolveJdkFqcnBody.includes('jdk_source_search_dirs')
      && resolveJdkFqcnBody.includes('spawn_jdk_warm_if_needed')
      && !resolveJdkFqcnBody.includes('ensure_jdk_navigation_sources'),
    'discovery: JDK go-to-definition does not synchronously extract src.zip',
  );

  ok(
    resolveFqcnJdkBody.includes('jdk_sources_ready')
      && resolveFqcnJdkBody.includes('spawn_jdk_warm_if_needed')
      && !resolveFqcnJdkBody.includes('ensure_jdk_navigation_sources'),
    'discovery: JDK FQCN candidate lookup is non-blocking when sources not ready',
  );

  ok(
    jdkSearchDirsBody.includes('.extracted')
      && !jdkSearchDirsBody.includes('ensure_jdk_navigation_sources'),
    'discovery: JDK search dirs only use already-extracted trees',
  );

  ok(
    classpathRs.includes('resolve_jdk_fqcn_location')
      && classpathRs.includes('is_java_util_simple_type')
      && resolveFqcnJdkBody.includes('java.util.'),
    'discovery: java.util.List and peers resolve via JDK sources',
  );

  ok(
    modRs.includes('find_external_definition_with_well_known')
      && modRs.includes('Some(true)')
      && modRs.includes('jdtls::find_definition'),
    'discovery: definition chain classpath-first then jdtls for Spring/libraries',
  );

  ok(
    classpathRs.includes('gradle_submodule_definition_finds_sibling_type')
      && classpathRs.includes('maven_submodule_definition_finds_sibling_type'),
    'discovery: rust tests cover Gradle and Maven sibling module navigation',
  );

  ok(
    classpathRs.includes('resolves_spring_type_from_dependency_sources')
      && classpathRs.includes('resolves_spring_type_from_maven_dependency_sources'),
    'discovery: rust tests cover Spring library navigation for Gradle and Maven',
  );

  ok(
    classpathRs.includes('resolves_java_lang_string_via_find_external_definition')
      && classpathRs.includes('resolves_java_util_list_from_materialized_jdk_sources')
      && classpathRs.includes('resolves_java_lang_string_maven_module'),
    'discovery: rust tests cover JDK String/List for Gradle and Maven',
  );

  ok(
    classpathRs.includes('warm_jdk_sources_uses_maven_reactor_root')
      && classpathRs.includes('warm_jdk_sources_gradle_settings_root'),
    'discovery: rust tests cover JDK warm scope for Maven reactor and Gradle settings',
  );

  ok(
    classpathRs.includes('jdk_navigation_skips_sync_extract_when_not_ready'),
    'discovery: rust test ensures F12 does not block on JDK zip extract',
  );

  ok(
    classpathRs.includes('library_source_dirs')
      && classpathRs.includes('entry.file_name() != "jdk"'),
    'discovery: Spring/library dirs exclude JDK tree (JDK has dedicated resolver)',
  );

  ok(
    classpathRs.includes('resolve_type_by_fqcn')
      && classpathRs.includes('resolve_library_type_location')
      && classpathRs.includes('org.springframework'),
    'discovery: Spring types still resolve through library source extraction path',
  );
}
