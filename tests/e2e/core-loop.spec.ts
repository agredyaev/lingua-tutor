// Core user journey (12-testing.md §7): profile → tutor context → import
// (validate → preview → apply) → review (reveal → grade → undo) → dashboard.
import { expect, test } from "@playwright/test";

test.describe.configure({ mode: "serial" });

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

  // Profile id from the API (UI stores it in context).
  const profiles = await page.request.get("/api/v1/language-profiles");
  const profileId = (await profiles.json())[0].id as string;

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
