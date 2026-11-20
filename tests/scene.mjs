import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";
import { chromium } from "playwright-core";

export async function verifyAssistant(
  page,
  screenshotPath,
  options = { isDesktop: false },
) {
  const main = page.getByRole("main");
  const button = (name) => page.getByRole("button", { name, exact: true });
  const state = (name) =>
    main.and(page.locator(`[data-state="${name}"]`)).waitFor();
  const message = page.getByRole("textbox", { name: "Mensagem", exact: true });

  const submit = async (text, result) => {
    await message.fill(text);
    await Promise.all([state("thinking"), button("Enviar").click()]);
    await state(result);
  };

  await state("idle");
  assert.equal(
    await message.evaluate((el) => el.tagName),
    "TEXTAREA",
  );
  await page.locator("canvas").first().waitFor();

  await button("Modo imersivo").click();
  await message.waitFor({ state: "hidden" });
  await button("Sair do modo imersivo").click();
  await message.waitFor();

  // History and persistent Memory drawer checks
  await button("Histórico da conversa").click();
  await page.getByRole("log", { name: "Histórico da conversa" }).waitFor();
  await page.getByRole("tab", { name: /^Memória/ }).click();
  const memInput = page.getByRole("textbox", { name: "Nova memória", exact: true });
  await memInput.fill("Preferência de teste E2E");
  await button("Salvar").click();
  await page.getByText("Preferência de teste E2E", { exact: true }).waitFor();
  await button("Remover memória").click();
  await page
    .getByText("Preferência de teste E2E", { exact: true })
    .waitFor({ state: "detached" });
  await button("Fechar histórico").click();
  await page.getByRole("dialog").waitFor({ state: "detached" });

  if (!options.isDesktop) {
    await submit("Olá", "responding");
    await state("idle");
    await submit("alerta", "alert");
  }

  const png = await page.screenshot({ path: screenshotPath });
  assert.equal(png.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  assert.ok(png.readUInt32BE(16) > 0 && png.readUInt32BE(20) > 0);

  await button("Ajustes").click();

  await page.getByRole("tab", { name: "IA", exact: true }).click();
  const providerSelect = page.getByRole("combobox", { name: "Provedor de IA" });
  await providerSelect.selectOption("chatgpt");
  await button("Entrar com ChatGPT (OAuth)").waitFor();
  await providerSelect.selectOption("gemini");
  await button("Conta Google (OAuth)").click();
  await button("Entrar com Google (OAuth)").waitFor();
  await button("Chave da API (AI Studio)").click();

  await page.getByRole("tab", { name: "Voz", exact: true }).click();
  const ttsEngineSelect = page.getByRole("combobox", { name: "Motor de voz" });
  assert.equal(await ttsEngineSelect.inputValue(), "elevenlabs");
  const replyModeSelect = page.getByRole("combobox", { name: "Economia de créditos" });
  assert.equal(await replyModeSelect.inputValue(), "brief");
  await page.getByRole("combobox", { name: "Modelo ElevenLabs" }).waitFor();
  await page.getByRole("textbox", { name: "Voice ID da ElevenLabs" }).waitFor();
  await button("Testar voz").waitFor();

  await page.getByRole("tab", { name: "Sistema", exact: true }).click();
  await page.getByRole("combobox", { name: "Modo de permissão" }).selectOption("review");

  await page.getByRole("tab", { name: "Visual", exact: true }).click();
  await page.getByLabel("Qualidade gráfica").selectOption("Low");
  await page.getByLabel("Reduzir movimento").check();
  await page.waitForTimeout(200);
  await button("Fechar ajustes").click();

  await page.reload();
  await state("idle");

  await button("Ajustes").click();
  await page.getByRole("tab", { name: "Sistema", exact: true }).click();
  assert.equal(
    await page.getByRole("combobox", { name: "Modo de permissão" }).inputValue(),
    "review",
  );
  await page.getByRole("tab", { name: "Visual", exact: true }).click();
  assert.equal(
    await page.getByLabel("Qualidade gráfica").inputValue(),
    "Low",
  );
  assert.ok(await page.getByLabel("Reduzir movimento").isChecked());
  await button("Fechar ajustes").click();

  // Verify dynamic contextual tabs are hidden by default when idle
  assert.equal(await page.getByRole("tab", { name: /^Agentes/ }).count(), 0);
  assert.equal(await page.getByRole("tab", { name: /^Navegador/ }).count(), 0);
  assert.equal(await page.getByRole("tab", { name: /^Tela/ }).count(), 0);

  if (!options.isDesktop) {
    // Trigger contextual browser split-pane via chat (Codex-style)
    await message.fill("abre o navegador embutido");
    await button("Enviar").click();
    await page.getByRole("textbox", { name: "Barra de endereço" }).waitFor();
    await page.getByRole("tab", { name: /^Navegador/ }).waitFor();
    await page.waitForTimeout(350);
    await page.screenshot({ path: "docs/assistant-browser.png" });

    // Close browser tab and verify it vanishes cleanly
    await button("Fechar navegador").click();
    await message.waitFor();
    assert.equal(await page.getByRole("tab", { name: /^Navegador/ }).count(), 0);

    // Trigger contextual agent spawn via chat and verify Agentes tab appears dynamically
    await state("idle");
    await message.fill("crie um agente para pesquisar IA");
    await button("Enviar").click();
    await page.getByRole("tab", { name: /^Agentes/ }).waitFor();
    await page.getByRole("tab", { name: /^Agentes/ }).click();
    await page.getByRole("heading", { name: "Central de Agentes" }).waitFor();
    await page.getByText("Concluído").waitFor();
    await page.waitForTimeout(200);
    await page.screenshot({ path: "docs/assistant-agents.png" });
    await page.getByRole("tab", { name: /^Núcleo/ }).click();
    await message.waitFor();
  }

  console.log(`Verified assistant states and saved ${screenshotPath}`);
}

async function runBrowserTest() {
  const { createServer } = await import("vite");
  const server = await createServer({
    server: { port: 0, host: "127.0.0.1" },
  });
  let browser;
  try {
    await server.listen();
    browser = await chromium.launch({
      channel: "chrome",
      args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
    });
    const page = await browser.newPage({
      viewport: { width: 1280, height: 720 },
    });
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("console", (msg) => {
      if (msg.type() === "error" && !msg.text().includes("404 (Not Found)")) {
        errors.push(msg.text());
      }
    });
    await page.goto(server.resolvedUrls.local[0]);
    await page
      .getByRole("heading", { name: /É um copiloto vivo no seu Windows/i })
      .waitFor();
    await page
      .getByRole("button", { name: /Abrir Simulador 3D ao Vivo/i })
      .click();
    await verifyAssistant(page, "docs/assistant.png", { isDesktop: false });
    assert.deepEqual(errors, []);
  } finally {
    await browser?.close();
    await server.close();
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  await runBrowserTest();
}
