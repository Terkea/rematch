/** Refuses to run destructive test setup against anything but a dedicated *_test database. */
export function assertSafeTestDatabase(testUrl: string | undefined, mainUrl?: string): string {
  if (!testUrl)
    throw new Error("DATABASE_URL_TEST is not set (run via docker compose run --rm worker)");
  const name = new URL(testUrl).pathname.replace(/^\//, "");
  if (!name.endsWith("_test")) {
    throw new Error(
      `Refusing to run tests: database "${name}" in DATABASE_URL_TEST does not end with _test`,
    );
  }
  if (mainUrl && mainUrl === testUrl) {
    throw new Error("Refusing to run tests: DATABASE_URL_TEST equals DATABASE_URL");
  }
  return testUrl;
}
