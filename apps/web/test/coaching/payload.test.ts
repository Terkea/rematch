import { reportSections, reports } from "@ci/db/generated";
import { eq } from "drizzle-orm";
import { beforeAll, describe, expect, it } from "vitest";
import { assembleReportPayload } from "../../src/server/coaching";
import { payloadFixture, resetCoachingTables, seedReport } from "../helpers/coaching";
import { resetGameTables, seedAccount } from "../helpers/games";
import { resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
let accountId: string;

beforeAll(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachingTables(db);
  ({ accountId } = await seedAccount(db, { username: "payload-player" }));
});

async function assembled(reportId: string) {
  const [report] = await db.select().from(reports).where(eq(reports.id, reportId));
  if (!report) throw new Error("no report");
  const sections = await db
    .select()
    .from(reportSections)
    .where(eq(reportSections.reportId, reportId));
  return { report, payload: assembleReportPayload(report, sections) };
}

const strip = ({ generatedAt, reportId, version, ...rest }: Record<string, unknown>) => rest;

describe("assembleReportPayload", () => {
  it("assembles the example payload", async () => {
    const { reportId } = await seedReport(db, accountId, {
      timeClass: "rapid",
      payload: "example",
    });
    const { report, payload } = await assembled(reportId ?? "");
    expect(strip(payload)).toEqual(strip(payloadFixture("example")));
    expect(payload.reportId).toBe(report.id);
    expect(payload.version).toBe(report.version);
    expect(payload.generatedAt).toBe(report.updatedAt.toISOString());
  });

  it("assembles the partial payload", async () => {
    const fixture = payloadFixture("partial");
    const { reportId } = await seedReport(db, accountId, {
      timeClass: "blitz",
      payload: "partial",
    });
    const { payload } = await assembled(reportId ?? "");
    expect(payload.status).toBe("partial");
    expect(payload.sections.openings).toBeNull();
    expect("areas" in payload.sections).toBe(false);
    expect(payload.dropped).toEqual(fixture.dropped);
    expect(strip(payload)).toEqual(strip(fixture));
  });

  it("drops the disclosure when no section is ready", async () => {
    const { reportId } = await seedReport(db, accountId, {
      timeClass: "bullet",
      payload: "partial",
    });
    await db
      .update(reportSections)
      .set({ status: "dropped", content: null })
      .where(eq(reportSections.reportId, reportId ?? ""));
    const { payload } = await assembled(reportId ?? "");
    expect(payload.status).toBe("computed_only");
    expect(payload.aiDisclosure).toBeNull();
    expect(Object.values(payload.sections).every((s) => s === null)).toBe(true);
  });
});
