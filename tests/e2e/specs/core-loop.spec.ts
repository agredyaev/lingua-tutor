// Core user journey (12-testing.md §7): profile → tutor context → import
// (validate → preview → apply) → review (reveal → grade → undo) → dashboard.
import { expect, test } from "@playwright/test";

test.describe.configure({ mode: "serial" });

async function rpc<T>(
  page: import("@playwright/test").Page,
  id: string,
  method: string,
  params: Record<string, unknown>,
): Promise<T> {
  const response = await page.request.post("/rpc", {
    data: { jsonrpc: "2.0", id, method, params },
  });
  expect(response.ok()).toBeTruthy();
  const payload = (await response.json()) as { result?: T; error?: unknown };
  expect(payload.error).toBeUndefined();
  return payload.result as T;
}

const lessonResult = (profileId: string) =>
  JSON.stringify({
    schema_version: "1",
    client_operation_id: `op-e2e-${Date.now()}`,
    language_profile_id: profileId,
    language_pack_id: "es-ES",
    language_pack_version: "0.2.0",
    lesson: { scenario: "pharmacy", duration_seconds: 900 },
    new_learning_items: [
      { item_type: "phrase", target_text: "¿Tiene algo para la tos?" },
    ],
    card_mutations: [
      {
        operation: "create",
        client_operation_id: `op-e2e-card-${Date.now()}`,
        card: {
          intent: "production",
          prompt: "Спросите что-нибудь от кашля",
          answer: "¿Tiene algo para la tos?",
          prompt_language: "ru",
          creation_reason: "e2e",
        },
      },
    ],
  });

test("app shell loads with navigation", async ({ page }) => {
  await page.goto("/dashboard");
  await expect(page.getByRole("heading", { name: "Dashboard" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Review" })).toBeVisible();
});

test("create profile, import lesson, review the card", async ({ page }) => {
  // Profile
  await page.goto("/languages");
  await page.getByRole("button", { name: /Start Spanish/ }).click();
  await expect(page.getByRole("button", { name: /es-ES/ })).toBeVisible();

  // Profile id through the same JSON-RPC transport used by the PWA.
  const profilesResponse = await page.request.post("/rpc", {
    data: {
      jsonrpc: "2.0",
      id: "e2e-list-profiles",
      method: "list_language_profiles",
      params: {},
    },
  });
  expect(profilesResponse.ok()).toBeTruthy();
  const profilesPayload = (await profilesResponse.json()) as {
    result: Array<{ id: string }>;
    error?: unknown;
  };
  expect(profilesPayload.error).toBeUndefined();
  const profileId = profilesPayload.result[0].id;

  // Import: paste → validate → preview → apply
  await page.goto("/import");
  await page.locator("textarea").fill(lessonResult(profileId));
  await page.getByRole("button", { name: "Validate" }).click();
  await expect(page.getByText(/Preview — pharmacy/)).toBeVisible();
  await expect(page.getByText("1 card mutations")).toBeVisible();
  await page.getByRole("button", { name: "Apply" }).click();
  await expect(page.getByRole("heading", { name: "Applied" })).toBeVisible();
  await expect(page.getByText(/applied/)).toBeVisible();

  // Review: reveal → grade Good → session end; undo brings the card back.
  await page.goto("/review");
  await expect(page.getByText("1 / 1")).toBeVisible();
  await page.getByRole("button", { name: /Reveal/ }).click();
  await expect(page.getByText("¿Tiene algo para la tos?")).toBeVisible();
  await page.getByRole("button", { name: /3 Good/ }).click();
  await expect(page.getByText(/Session done/)).toBeVisible();
  await page.getByRole("button", { name: "Undo last grade" }).click();
  await expect(page.getByText("1 / 1")).toBeVisible();
  await page.getByRole("button", { name: /Reveal/ }).click();
  await page.getByRole("button", { name: /4 Easy/ }).click();
  await expect(page.getByText(/Session done/)).toBeVisible();

  // Dashboard reflects the day.
  await page.goto("/dashboard");
  await expect(page.getByText(/reviews today/)).toBeVisible();
});


test("real lesson lifecycle creates a review card and advances the next lesson", async ({ page }) => {
  const profiles = await rpc<Array<{ id: string }>>(
    page,
    "e2e-cycle-profiles",
    "list_language_profiles",
    {},
  );
  expect(profiles).toHaveLength(1);
  const profileId = profiles[0].id;

  const nextBefore = await rpc<{ lesson_number: number }>(
    page,
    "e2e-cycle-next-before",
    "get_next_lesson",
    { language_profile_id: profileId },
  );

  const started = await rpc<{ id: string; lesson_number: number }>(
    page,
    "e2e-cycle-start",
    "start_lesson",
    {
      language_profile_id: profileId,
      scenario: "restaurant",
      planned_goal: "order a simple meal",
    },
  );
  expect(started.lesson_number).toBe(nextBefore.lesson_number);

  const op = Date.now();
  const completed = await rpc<{ lesson_id: string }>(
    page,
    "e2e-cycle-complete",
    "complete_lesson",
    {
      lesson_id: started.id,
      lesson_result: {
        schema_version: "1",
        client_operation_id: `op-e2e-cycle-${op}`,
        language_profile_id: profileId,
        language_pack_id: "es-ES",
        language_pack_version: "0.2.0",
        lesson: {
          scenario: "restaurant",
          duration_seconds: 720,
          tutor_summary: "Practised ordering a meal.",
          next_goal: "ask for the bill",
        },
        metrics: {
          speaking_seconds: 300,
          unaided_recall_count: 4,
          aided_recall_count: 1,
          listening_score: 0.8,
          avg_answer_millis: 1800,
          long_pause_count: 1,
          confidence: 0.75,
        },
        new_learning_items: [
          {
            client_item_key: "bill",
            item_type: "phrase",
            target_text: "La cuenta, por favor.",
          },
        ],
        evidence: [
          {
            client_item_key: "bill",
            kind: "prompted_production",
            success: true,
            assistance_level: 1,
            context: "restaurant:bill",
          },
        ],
        card_mutations: [
          {
            operation: "create",
            client_operation_id: `op-e2e-cycle-card-${op}`,
            card: {
              intent: "production",
              prompt: "Попросите счёт",
              answer: "La cuenta, por favor.",
              prompt_language: "ru",
              creation_reason: "e2e lesson lifecycle",
            },
          },
        ],
      },
    },
  );
  expect(completed.lesson_id).toBe(started.id);

  const cards = await rpc<Array<{ answer: string; status: string }>>(
    page,
    "e2e-cycle-cards",
    "list_cards",
    { language_profile_id: profileId },
  );
  expect(cards.some((card) => card.answer === "La cuenta, por favor.")).toBeTruthy();

  // Real browser review UI consumes the card produced by complete_lesson.
  await page.goto("/review");
  await expect(page.getByText("1 / 1")).toBeVisible();
  await expect(page.getByText("Попросите счёт")).toBeVisible();
  await page.getByRole("button", { name: /Reveal/ }).click();
  await expect(page.getByText("La cuenta, por favor.")).toBeVisible();
  await page.getByRole("button", { name: /3 Good/ }).click();
  await expect(page.getByText(/Session done/)).toBeVisible();

  const nextAfter = await rpc<{ lesson_number: number; previous_scenario?: string }>(
    page,
    "e2e-cycle-next-after",
    "get_next_lesson",
    { language_profile_id: profileId },
  );
  expect(nextAfter.lesson_number).toBe(started.lesson_number + 1);
  expect(nextAfter.previous_scenario).toBe("restaurant");

  // User-facing Learn page must expose the same next lesson.
  await page.goto("/learn");
  await expect(
    page.getByText(`Lesson #${nextAfter.lesson_number}`),
  ).toBeVisible();
});


test("card can be suspended and restored through the browser", async ({ page }) => {
  await page.goto("/cards");
  await page.getByText("Попросите счёт").click();
  await expect(page.getByRole("heading", { name: "Попросите счёт" })).toBeVisible();

  await page.getByRole("button", { name: "Suspend" }).click();
  await expect(page.getByText(/status suspended/)).toBeVisible();

  await page.getByRole("button", { name: "Restore" }).click();
  await expect(page.getByText(/status review/)).toBeVisible();
});

test("offline review grade is queued and replayed after reconnect", async ({ page }) => {
  const profiles = await rpc<Array<{ id: string }>>(
    page,
    "e2e-offline-profiles",
    "list_language_profiles",
    {},
  );
  const profileId = profiles[0].id;

  const started = await rpc<{ id: string; lesson_number: number }>(
    page,
    "e2e-offline-start",
    "start_lesson",
    {
      language_profile_id: profileId,
      scenario: "cafe",
      planned_goal: "order water",
    },
  );
  const op = Date.now();
  await rpc<{ lesson_id: string }>(
    page,
    "e2e-offline-complete",
    "complete_lesson",
    {
      lesson_id: started.id,
      lesson_result: {
        schema_version: "1",
        client_operation_id: `op-e2e-offline-${op}`,
        language_profile_id: profileId,
        language_pack_id: "es-ES",
        language_pack_version: "0.2.0",
        lesson: {
          scenario: "cafe",
          duration_seconds: 480,
          tutor_summary: "Practised ordering water.",
          next_goal: "ask for sparkling water",
        },
        new_learning_items: [
          {
            client_item_key: "water",
            item_type: "phrase",
            target_text: "Agua, por favor.",
          },
        ],
        evidence: [
          {
            client_item_key: "water",
            kind: "prompted_production",
            success: true,
            assistance_level: 1,
            context: "cafe:water",
          },
        ],
        card_mutations: [
          {
            operation: "create",
            client_operation_id: `op-e2e-offline-card-${op}`,
            card: {
              intent: "production",
              prompt: "Попросите воды",
              answer: "Agua, por favor.",
              prompt_language: "ru",
              creation_reason: "offline replay e2e",
            },
          },
        ],
      },
    },
  );

  // Online visit creates a real session and caches its prefetched queue.
  await page.goto("/review");
  await expect(page.getByText("Попросите воды")).toBeVisible();

  // Simulate loss of the local runtime only after the queue is cached.
  await page.route("**/rpc", (route) => route.abort());
  await page.reload();
  await expect(page.getByText("Попросите воды")).toBeVisible();
  await page.getByRole("button", { name: /Reveal/ }).click();
  await page.getByRole("button", { name: /3 Good/ }).click();
  await expect(page.getByText(/Session done/)).toBeVisible();

  const pendingBeforeReconnect = await page.evaluate(() => {
    const raw = localStorage.getItem("lt-offline-events") ?? "[]";
    return JSON.parse(raw).length;
  });
  expect(pendingBeforeReconnect).toBe(1);

  await page.unroute("**/rpc");
  await page.reload();

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const raw = localStorage.getItem("lt-offline-events") ?? "[]";
        return JSON.parse(raw).length;
      }),
    )
    .toBe(0);

  const summary = await rpc<{ reviews_today: number }>(
    page,
    "e2e-offline-summary",
    "get_review_summary",
    { language_profile_id: profileId },
  );
  expect(summary.reviews_today).toBeGreaterThan(0);
});


test("full export contains the current profile state", async ({ page }) => {
  const profiles = await rpc<Array<{ id: string }>>(
    page,
    "e2e-export-profiles",
    "list_language_profiles",
    {},
  );
  const profileId = profiles[0].id;

  await page.goto("/export");
  const full = page.getByRole("link", { name: "Full JSON export" });
  await expect(full).toHaveAttribute(
    "href",
    `/api/v1/export/full?language_profile_id=${profileId}`,
  );

  const response = await page.request.get(
    `/api/v1/export/full?language_profile_id=${profileId}`,
  );
  expect(response.ok()).toBeTruthy();
  const exported = (await response.json()) as {
    schema_version: string;
    language_profile_id: string;
    lessons: unknown[];
    cards: Array<{ prompt: string }>;
    review_events: unknown[];
  };
  expect(exported.schema_version).toBe("1");
  expect(exported.language_profile_id).toBe(profileId);
  expect(exported.lessons.length).toBeGreaterThan(0);
  expect(exported.cards.some((card) => card.prompt === "Попросите счёт")).toBeTruthy();
  expect(exported.review_events.length).toBeGreaterThan(0);
});
