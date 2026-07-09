/**
 * Compiler settings regression — Maven/Gradle path validation and installed pickers.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function extractFunctionBody(src, name) {
  const re = new RegExp(`function ${name}\\([^)]*\\)\\s*\\{`);
  const m = src.match(re);
  if (!m) return '';
  const start = m.index + m[0].length;
  let depth = 1;
  for (let i = start; i < src.length; i += 1) {
    const ch = src[i];
    if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) return src.slice(start, i);
    }
  }
  return '';
}

export function testCompilerSettingsRegression(appSrc, toolchainSrc, mavenRsSrc, ok) {
  const orderBody = extractFunctionBody(appSrc, 'loadCompilersSettingsSection');
  ok(orderBody.includes("'maven'"), 'compiler settings: maven in COMPILER_ORDER');
  ok(
    orderBody.indexOf("'gradle'") >= 0 && orderBody.indexOf("'maven'") > orderBody.indexOf("'gradle'"),
    'compiler settings: maven listed after gradle',
  );

  const statusBody = orderBody;
  ok(statusBody.includes('tool.path_error'), 'compiler settings: status checks path_error first');
  ok(statusBody.includes("'invalid'"), 'compiler settings: invalid badge for bad path');

  const rowBody = orderBody;
  ok(rowBody.includes('mavenInstalled'), 'compiler settings: render row accepts mavenInstalled');
  ok(rowBody.includes('settings-compiler-maven-select'), 'compiler settings: maven installed picker');
  ok(rowBody.includes('settings-compiler-gradle-select'), 'compiler settings: gradle installed picker');
  ok(rowBody.includes('ij-compiler-error'), 'compiler settings: shows path error message');
  ok(rowBody.includes('gradlew'), 'compiler settings: gradle wrapper precedence note');
  ok(rowBody.includes('mvnw'), 'compiler settings: maven wrapper precedence note');
  ok(rowBody.includes('MAVEN_HOME'), 'compiler settings: maven path placeholder');

  const bindBody = orderBody;
  ok(bindBody.includes('settings-compiler-maven-select'), 'compiler settings: maven select bound');

  ok(orderBody.includes('maven_installed'), 'compiler settings: loads maven_installed from API');
  ok(orderBody.includes('settings-java-release-select'), 'compiler settings: Java language level picker');
  ok(orderBody.includes('JAVA_RELEASE_LEVELS'), 'compiler settings: Java language level options');
  ok(orderBody.includes('/api/settings/jdk'), 'compiler settings: language level saves via jdk API');

  const settingsSrc = fs.readFileSync(path.join(ROOT, 'src/settings/mod.rs'), 'utf8');
  ok(settingsSrc.includes('java_release: Option<u32>'), 'settings: persists java_release');
  ok(settingsSrc.includes('set_java_release'), 'settings: set_java_release API');

  const javaDiagSrc = fs.readFileSync(path.join(ROOT, 'src/workspace/java_diagnostics.rs'), 'utf8');
  ok(javaDiagSrc.includes('java_release_from_gradle_tree'), 'java diagnostics: walks Gradle tree for release');
  ok(javaDiagSrc.includes('configured_java_release'), 'java diagnostics: uses configured java_release fallback');
  ok(javaDiagSrc.includes('javac_release_for_path'), 'java diagnostics: exposes resolved javac release');

  const classpathRsSrc = fs.readFileSync(path.join(ROOT, 'src/workspace/classpath.rs'), 'utf8');
  ok(classpathRsSrc.includes('workspace_sibling_module_classpath'), 'classpath: merges sibling module outputs');
  ok(classpathRsSrc.includes('gradle_project_dependency_dirs'), 'classpath: resolves Gradle project() deps');
  ok(classpathRsSrc.includes('supplement_jakarta_validation_api'), 'classpath: supplements jakarta.validation-api for @Valid');

  const gradleRsSrc = fs.readFileSync(path.join(ROOT, 'src/workspace/gradle.rs'), 'utf8');
  ok(gradleRsSrc.includes('parse_gradle_project_dependency_paths'), 'gradle: parses project() dependency paths');

  const initGradleSrc = fs.readFileSync(path.join(ROOT, 'gradle/reaper-classpath.init.gradle'), 'utf8');
  ok(initGradleSrc.includes('f.isDirectory()'), 'gradle init: emits CLASSES for project dependency dirs');

  const langCtxSrc = fs.readFileSync(path.join(ROOT, 'src/workspace/language_compiler_context.rs'), 'utf8');
  ok(langCtxSrc.includes('configured_java_release'), 'language context: exposes configured_java_release');

  ok(toolchainSrc.includes('id: "maven"'), 'compiler settings: maven tool in TOOLS');
  ok(toolchainSrc.includes('REAPER_MVN'), 'compiler settings: maven env key');
  ok(toolchainSrc.includes('path_error'), 'compiler settings: API exposes path_error');
  ok(toolchainSrc.includes('maven_installed'), 'compiler settings: API exposes maven_installed');
  ok(toolchainSrc.includes('normalize_maven_binary'), 'compiler settings: maven path normalization');
  ok(toolchainSrc.includes('normalize_gradle_binary'), 'compiler settings: gradle path normalization');

  ok(mavenRsSrc.includes('list_installed_mavens'), 'compiler settings: maven install scanner');
  ok(mavenRsSrc.includes('validate_maven_path'), 'compiler settings: maven path validation');
}

export function testCompilerStatusSimulation(ok) {
  function compilerStatus(tool) {
    if (tool.path_error) {
      return { cls: 'invalid', label: 'Not found' };
    }
    if (tool.configured) {
      return { cls: 'custom', label: 'Custom' };
    }
    if (tool.effective) {
      return { cls: 'ready', label: 'PATH' };
    }
    return { cls: 'missing', label: 'Missing' };
  }

  ok(compilerStatus({ path_error: 'gradle path not found: /bad' }).cls === 'invalid', 'compiler sim: path_error → invalid');
  ok(compilerStatus({ configured: true, effective: '/opt/bin/gradle' }).cls === 'custom', 'compiler sim: configured → custom');
  ok(compilerStatus({ effective: '/opt/bin/mvn' }).cls === 'ready', 'compiler sim: effective → ready');
  ok(compilerStatus({}).cls === 'missing', 'compiler sim: no path → missing');
}
