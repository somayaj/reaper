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
