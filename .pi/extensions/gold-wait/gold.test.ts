import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { parsePr, waitForGold } from "./gold.ts";

const url = "https://github.com/Mokto/opengeocoding/pull/181";
const target = "https://github.com/Mokto/opengeocoding/actions/runs/123";
const started = "2026-10-18T11:00:00Z";
const updated = "2026-10-18T11:30:00Z";

function harness() {
	let head = "a".repeat(40);
	let state = "open";
	let report =
		"Quality table\n<!-- speed: pass -->\n## Speed: no slowdown over 15%\n<!-- speed end -->\n";
	const statuses = ["gold-full", "gold-speed"].map((context, id) => ({
		context,
		id,
		state: "success",
		description: "done",
		target_url: target,
		updated_at: updated,
	}));
	const workflow = {
		status: "completed",
		run_started_at: started,
		run_attempt: 1,
	};
	const downloads: string[] = [];
	const run = async (args: string[]) => {
		if (args[0] === "run") {
			downloads.push(args[args.indexOf("--name") + 1]);
			await writeFile(
				join(args[args.indexOf("--dir") + 1], "report.md"),
				report,
			);
			return "";
		}
		const path = args.at(-1);
		if (path?.endsWith("pulls/181"))
			return JSON.stringify({ state, head: { sha: head } });
		if (path?.includes("/status?")) return JSON.stringify({ statuses });
		if (path?.endsWith("actions/runs/123")) return JSON.stringify(workflow);
		throw new Error(`Unexpected command: ${args.join(" ")}`);
	};
	return {
		run,
		statuses,
		workflow,
		downloads,
		setHead(value: string) {
			head = value;
		},
		setState(value: string) {
			state = value;
		},
		setReport(value: string) {
			report = value;
		},
	};
}

test("validates the PR URL before executing gh", async () => {
	assert.deepEqual(parsePr(url), {
		repo: "Mokto/opengeocoding",
		number: "181",
	});
	for (const bad of [
		"https://evil.test/Mokto/opengeocoding/pull/181",
		`${url}?foo=bar`,
		"https://github.com/../repo/pull/1",
		`${url};echo hello`,
	]) {
		await assert.rejects(waitForGold(bad), /Expected https/);
	}
});

test("returns both verdicts and the exact head/run/attempt artifact", async () => {
	const h = harness();
	const result = await waitForGold(url, { run: h.run });
	assert.equal(result.outcome, "passed");
	assert.match(result.report ?? "", /Speed: no slowdown/);
	assert.deepEqual(h.downloads, [`gold-${"a".repeat(40)}-123-1`]);
});

test("waits for BOTH checks, follows pushes, emits progress", async () => {
	const h = harness();
	h.statuses[1].state = "pending";
	let polls = 0;
	const result = await waitForGold(url, {
		run: h.run,
		pollMs: 1,
		onProgress: () => {
			if (++polls === 1) h.setHead("b".repeat(40));
			else h.statuses[1].state = "success";
		},
	});
	assert.equal(result.outcome, "passed");
	assert.equal(result.head, "b".repeat(40));
	assert.equal(h.downloads.length, 1);
	assert.ok(polls >= 3);
});

test("does not return an artifact if the head changed during download", async () => {
	const h = harness();
	let changed = false;
	const result = await waitForGold(url, {
		run: async (args) => {
			const output = await h.run(args);
			if (args[0] === "run" && !changed) {
				changed = true;
				h.setHead("b".repeat(40));
			}
			return output;
		},
	});
	assert.equal(result.head, "b".repeat(40));
	assert.equal(h.downloads.length, 2);
});

test("quality failure, worse failure class and speed failure are regressions", async () => {
	for (const kind of ["quality", "speed", "gate"]) {
		const h = harness();
		if (kind === "quality") h.statuses[0].state = "failure";
		if (kind === "speed")
			h.setReport(
				"<!-- speed: fail -->\n## Speed: SearchAddress slower, total +25%\n",
			);
		if (kind === "gate")
			h.setReport(
				"<!-- speed: pass -->\n## Worse failure class (1)\nGate: FAIL (1 with a worse failure class).\n",
			);
		assert.equal(
			(await waitForGold(url, { run: h.run })).outcome,
			"regressions",
		);
	}
});

test("unmeasured speed cannot pass even when GitHub marks it success", async () => {
	for (const report of [
		"quality only",
		"<!-- speed: skip -->",
		"<!-- speed: none -->",
		"",
	]) {
		const h = harness();
		h.setReport(report);
		assert.equal((await waitForGold(url, { run: h.run })).outcome, "blocked");
	}
});

test("stale statuses from a previous attempt are not downloaded", async () => {
	const h = harness();
	h.workflow.run_started_at = "2026-10-18T12:00:00Z";
	h.workflow.run_attempt = 2;
	const result = await waitForGold(url, {
		run: h.run,
		pollMs: 1,
		timeoutMs: 15,
	});
	assert.equal(result.outcome, "blocked");
	assert.equal(h.downloads.length, 0);
});

test("closed PRs, infrastructure errors and mismatched runs block", async () => {
	for (const kind of ["closed", "error", "mismatch"]) {
		const h = harness();
		if (kind === "closed") h.setState("closed");
		if (kind === "error") h.statuses[0].state = "error";
		if (kind === "mismatch") h.statuses[1].target_url = `${target}4`;
		assert.equal((await waitForGold(url, { run: h.run })).outcome, "blocked");
		assert.equal(h.downloads.length, 0);
	}
});

test("artifact access failure is blocked; user cancellation propagates", async () => {
	const h = harness();
	const result = await waitForGold(url, {
		run: async (args) => {
			if (args[0] === "run") throw new Error("artifact expired");
			return h.run(args);
		},
	});
	assert.equal(result.outcome, "blocked");
	assert.match(result.text, /artifact expired/);
	const controller = new AbortController();
	controller.abort();
	await assert.rejects(
		waitForGold(url, { run: h.run, signal: controller.signal }),
		{ name: "AbortError" },
	);
});
