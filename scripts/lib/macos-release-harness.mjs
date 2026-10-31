/**
 * macOS release pipeline regression — signing, jdtls warm, notarization prep.
 */

/**
 * @param {string} signSh
 * @param {string} copyJdtlsSh
 * @param {string} warmSh
 * @param {string} releaseSh
 * @param {string} releaseReadme
 * @param {(pass: boolean, label: string) => void} ok
 */
export function testMacosReleaseStaticRegression(
  signSh,
  copyJdtlsSh,
  warmSh,
  releaseSh,
  releaseReadme,
  ok,
) {
  ok(
    signSh.includes('warm-bundled-jdtls-config.sh'),
    'release: sign-macos-app warms jdtls before every codesign',
  );
  ok(
    /warm-bundled-jdtls-config\.sh"\s+"\$APP"/.test(signSh),
    'release: jdtls warm runs against the app bundle being signed',
  );
  ok(
    copyJdtlsSh.includes('rm -rf "$DEST/configuration"'),
    'release: bundled jdtls copy drops stale OSGi configuration cache',
  );
  ok(
    warmSh.includes('configuration/org.eclipse.osgi'),
    'release: warm script generates jdtls configuration/org.eclipse.osgi',
  );
  ok(
    warmSh.includes('rm -rf "$JDTLS/configuration"'),
    'release: warm script clears configuration before regenerating',
  );
  ok(
    releaseSh.includes('notarize-macos-dmg.sh') && releaseSh.includes('REAPER_NOTARIZE'),
    'release: release-macos.sh supports notarization when REAPER_NOTARIZE=1',
  );
  ok(
    releaseSh.includes('build-macos-intel-dmg.sh') || releaseSh.includes('x86_64'),
    'release: release-macos.sh documents Intel arch path',
  );
  ok(
    releaseReadme.includes('warm-bundled-jdtls-config.sh'),
    'release: release-notes README documents repeatable jdtls warm step',
  );
}
