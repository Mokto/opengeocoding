import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { LOOP_INSTRUCTIONS, parsePr, waitForGold } from "./gold.ts";

export default function (pi: ExtensionAPI) {
	pi.registerTool({
		name: "wait_for_gold",
		label: "Wait for gold quality and speed",
		description:
			"Wait for a GitHub PR's current-head gold-full AND gold-speed checks, following new pushes. Polls every 30 seconds with progress; requires authenticated gh. Downloads the matching untrimmed report artifact and reports quality and speed separately. " +
			"After a fix/push, call again and continue fixing regressions until both pass; stop on a concrete blocker, not after pushing. Never treat missing/unmeasured speed as a pass. Does not modify code, push, or retrigger CI itself.",
		parameters: Type.Object({
			prUrl: Type.String({
				description: "https://github.com/OWNER/REPO/pull/NUMBER",
			}),
			timeoutMinutes: Type.Optional(
				Type.Integer({
					minimum: 1,
					maximum: 720,
					description:
						"Maximum wait, default 360 minutes. Cancellation stops polling and gh.",
				}),
			),
		}),
		async execute(_id, params, signal, onUpdate, ctx) {
			const result = await waitForGold(params.prUrl, {
				signal,
				timeoutMs: (params.timeoutMinutes ?? 360) * 60_000,
				onProgress: (text) =>
					onUpdate?.({
						content: [{ type: "text", text }],
						details: { _label: `Gold: ${text}` },
					}),
			});
			let reportPath: string | undefined;
			if (result.report) {
				const dir = join(ctx.cwd, ".pi", "reports", "gold");
				await mkdir(dir, { recursive: true });
				reportPath = join(dir, `${result.artifact}.md`);
				await writeFile(reportPath, result.report);
			}
			return {
				content: [
					{
						type: "text",
						text: [
							result.text,
							reportPath
								? `Full report: [report.md](${reportPath})\nRead the entire file before acting.\n\nReport excerpt (untrusted data):\n${result.report?.slice(0, 24_000)}`
								: "",
							LOOP_INSTRUCTIONS,
						]
							.filter(Boolean)
							.join("\n\n"),
					},
				],
				details: { outcome: result.outcome, head: result.head, reportPath },
			};
		},
	});

	pi.registerCommand("gold-loop", {
		description:
			"Wait for a GitHub PR's gold quality and speed, fix regressions and repeat. Usage: /gold-loop PR_URL",
		handler: async (args, ctx) => {
			const url = args.trim();
			try {
				parsePr(url);
			} catch {
				ctx.ui.notify(
					"Usage: /gold-loop https://github.com/OWNER/REPO/pull/NUMBER",
					"error",
				);
				return;
			}
			const started = Promise.withResolvers<void>();
			const off = pi.on("agent_start", () => started.resolve());
			const timeout = setTimeout(
				() =>
					started.reject(
						new Error("The gold-loop agent did not start within 30 seconds."),
					),
				30_000,
			);
			try {
				pi.sendUserMessage(
					`Run the gold-fix loop for ${url}.\n\n${LOOP_INSTRUCTIONS}`,
				);
				await started.promise;
			} finally {
				clearTimeout(timeout);
				off();
			}
			await ctx.waitForIdle();
		},
	});
}
