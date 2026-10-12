<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { m } from '$lib/paraglide/messages.js';
	import type {
		AppSettings,
		ConnectionDraft,
		ConnectionSummary,
		QueryResult,
		TreeNode,
		WorkspaceTab
	} from '$lib/schemas/dbms';
	import {
		applyEdits,
		compareSchemas,
		deleteConnection,
		exportBackup,
		exportConnectionPack,
		exportResults,
		generateDdl,
		getConnection,
		getErd,
		getSettings,
		importConnectionPack,
		listConnections,
		listHistory,
		listTree,
		openSession,
		parseIpcError,
		runQuery,
		saveConnection,
		saveSettings,
		tablePage,
		closeSession,
		resultDiff,
		addBookmark
	} from '$lib/tool/dbms-ipc';
	import { subscribeDbmsMenu, type DbmsMenuAction } from '$lib/tool/dbms-menu';
	import { subscribeDbmsRefresh } from '$lib/tool/dbms-refresh';
	import WashIcon from '$lib/tool/WashIcon.svelte';
	import { washIcons } from '$lib/tool/wash-icons';
	import WashCodeEditor from '$lib/component/editor/WashCodeEditor.svelte';
	import ConnectionWizard from '$lib/component/wizard/ConnectionWizard.svelte';

	let connections = $state<ConnectionSummary[]>([]);
	let settings = $state<AppSettings | null>(null);
	let wizardOpen = $state(false);
	let wizardDraft = $state<ConnectionDraft | null>(null);
	let sessions = $state<Record<string, string>>({});
	let trees = $state<Record<string, TreeNode>>({});
	let selectedConnectionId = $state<string | null>(null);
	let filter = $state('');
	let showNav = $state(true);
	let showInspector = $state(true);
	let tabs = $state<WorkspaceTab[]>([
		{ id: 'console-1', kind: 'console', title: 'Console 1', sql: 'SELECT 1;' }
	]);
	let activeTabId = $state('console-1');
	let results = $state<QueryResult | null>(null);
	let priorResult = $state<QueryResult | null>(null);
	let bottomTab = $state<'results' | 'messages' | 'plan'>('results');
	let toast = $state<{ tone: string; text: string } | null>(null);
	let busy = $state(false);
	let inspectorText = $state('');
	let dataRows = $state<QueryResult | null>(null);
	let pendingEdits = $state<
		Array<{
			schema?: string | null;
			table: string;
			pk: Record<string, unknown>;
			column: string;
			newValue: unknown;
		}>
	>([]);
	let erd = $state<{
		nodes: Array<{ id: string; label: string; columns: string[] }>;
		edges: Array<{ from: string; to: string; label: string }>;
	} | null>(null);
	let compareOut = $state<{
		onlyLeft: string[];
		onlyRight: string[];
		differing: string[];
		migrationSql: string;
	} | null>(null);
	let history = $state<
		Array<{
			id: string;
			sqlText: string;
			status: string;
			durationMs?: number | null;
			createdAt: string;
		}>
	>([]);
	let onboardingOpen = $state(false);
	let aboutOpen = $state(false);
	let dataFilter = $state('');
	let consoleSql = $state('SELECT 1;');

	const activeTab = $derived(tabs.find((t) => t.id === activeTabId) ?? tabs[0]);

	let lastTabId = $state('');
	$effect(() => {
		if (activeTabId !== lastTabId) {
			lastTabId = activeTabId;
			const tab = tabs.find((t) => t.id === activeTabId);
			if (tab?.kind === 'console') consoleSql = tab.sql;
		}
	});

	$effect(() => {
		const sql = consoleSql;
		const tab = tabs.find((t) => t.id === activeTabId);
		if (tab?.kind === 'console' && tab.sql !== sql) {
			tab.sql = sql;
			tabs = [...tabs];
		}
	});

	function showToast(tone: string, text: string) {
		toast = { tone, text };
		setTimeout(() => (toast = null), 4000);
	}

	async function reload() {
		try {
			connections = await listConnections();
			settings = await getSettings();
			onboardingOpen = !(settings?.onboardingComplete ?? false);
			history = await listHistory(100);
		} catch (err) {
			showToast('error', parseIpcError(err).message);
		}
	}

	onMount(() => {
		void reload();
		const offMenu = subscribeDbmsMenu((action) => void handleMenu(action));
		const offRefresh = subscribeDbmsRefresh(() => void reloadAndTrees());
		return () => {
			offMenu();
			offRefresh();
		};
	});

	onDestroy(() => {
		for (const sessionId of Object.values(sessions)) {
			void closeSession(sessionId);
		}
	});

	async function reloadAndTrees() {
		await reload();
		for (const [connectionId, sessionId] of Object.entries(sessions)) {
			try {
				trees[connectionId] = await listTree(sessionId);
			} catch {
				/* ignore */
			}
		}
		trees = { ...trees };
	}

	async function handleMenu(action: DbmsMenuAction) {
		switch (action.type) {
			case 'new-connection':
				wizardDraft = null;
				wizardOpen = true;
				break;
			case 'new-console': {
				const id = `console-${crypto.randomUUID().slice(0, 8)}`;
				tabs = [
					...tabs,
					{
						id,
						kind: 'console',
						title: `Console ${tabs.length + 1}`,
						sql: '',
						connectionId: selectedConnectionId ?? undefined,
						sessionId: selectedConnectionId ? sessions[selectedConnectionId] : undefined
					}
				];
				activeTabId = id;
				break;
			}
			case 'refresh-tree':
				await reloadAndTrees();
				break;
			case 'run':
				await executeActive(false, false);
				break;
			case 'run-script':
				await executeActive(false, false);
				break;
			case 'explain':
				await executeActive(true, false);
				break;
			case 'history':
				openSpecial({ id: 'history', kind: 'history', title: m.history_title() });
				history = await listHistory(200);
				break;
			case 'settings':
				openSpecial({ id: 'settings', kind: 'settings', title: m.settings_title() });
				break;
			case 'erd':
				await openErd();
				break;
			case 'schema-compare':
				openSpecial({ id: 'compare', kind: 'compare', title: m.compare_title() });
				break;
			case 'export-results':
				await doExport('csv');
				break;
			case 'export-backup': {
				const json = await exportBackup();
				downloadText('dbms-backup.json', json);
				showToast('success', m.toast_exported());
				break;
			}
			case 'export-pack':
				await doExportPack();
				break;
			case 'import-pack':
				await doImportPack();
				break;
			case 'toggle-navigator':
				showNav = !showNav;
				break;
			case 'toggle-inspector':
				showInspector = !showInspector;
				break;
			case 'onboarding':
				onboardingOpen = true;
				break;
			case 'about':
				aboutOpen = true;
				break;
			case 'generate-ddl':
				await doGenerateDdl();
				break;
			case 'open-table':
				await openSelectedTable();
				break;
			case 'result-diff':
				if (priorResult && results) {
					const diff = await resultDiff(priorResult, results);
					inspectorText = JSON.stringify(diff, null, 2);
					showInspector = true;
				}
				break;
			case 'disconnect': {
				const connId = selectedConnectionId;
				const sessionId = connId ? sessions[connId] : undefined;
				if (connId && sessionId) {
					await closeSession(sessionId);
					delete sessions[connId];
					sessions = { ...sessions };
					delete trees[connId];
					trees = { ...trees };
				}
				break;
			}
		}
	}

	function openSpecial(tab: WorkspaceTab) {
		if (!tabs.some((t) => t.id === tab.id)) tabs = [...tabs, tab];
		activeTabId = tab.id;
	}

	async function connect(id: string) {
		busy = true;
		try {
			const sessionId = await openSession(id);
			sessions[id] = sessionId;
			sessions = { ...sessions };
			trees[id] = await listTree(sessionId);
			trees = { ...trees };
			selectedConnectionId = id;
			showToast('success', m.toast_connected());
			const consoleTab = tabs.find((t) => t.kind === 'console');
			if (consoleTab && consoleTab.kind === 'console') {
				consoleTab.connectionId = id;
				consoleTab.sessionId = sessionId;
				tabs = [...tabs];
			}
		} catch (err) {
			showToast('error', parseIpcError(err).message);
		} finally {
			busy = false;
		}
	}

	async function executeActive(explain: boolean, analyze: boolean) {
		const tab = activeTab;
		if (!tab || tab.kind !== 'console') return;
		const sessionId =
			tab.sessionId ?? (selectedConnectionId ? sessions[selectedConnectionId] : undefined);
		if (!sessionId) {
			showToast('error', 'Open a connection first.');
			return;
		}
		let confirmDestructive = true;
		if (settings?.confirmDestructive) {
			const upper = tab.sql.toUpperCase();
			if (['DROP ', 'DELETE ', 'TRUNCATE ', 'ALTER ', 'UPDATE '].some((k) => upper.includes(k))) {
				confirmDestructive = window.confirm(m.destructive_confirm());
				if (!confirmDestructive) return;
			}
		}
		busy = true;
		try {
			if (results) priorResult = results;
			results = await runQuery(
				{
					sessionId,
					sql: tab.sql,
					maxRows: settings?.defaultMaxRows ?? 500,
					explain,
					analyze
				},
				tab.connectionId,
				confirmDestructive
			);
			bottomTab = explain ? 'plan' : 'results';
			history = await listHistory(100);
		} catch (err) {
			const parsed = parseIpcError(err);
			if (parsed.code === 'DESTRUCTIVE_CONFIRM') {
				if (window.confirm(parsed.message)) {
					results = await runQuery(
						{
							sessionId,
							sql: tab.sql,
							maxRows: settings?.defaultMaxRows ?? 500,
							explain,
							analyze
						},
						tab.connectionId,
						true
					);
				}
			} else {
				showToast('error', parsed.message);
				bottomTab = 'messages';
				results = {
					columns: [],
					rows: [],
					rowCount: 0,
					truncated: false,
					durationMs: 0,
					messages: [parsed.message],
					explainText: null
				};
			}
		} finally {
			busy = false;
		}
	}

	async function onSaveConnection(draft: ConnectionDraft) {
		await saveConnection(draft);
		await reload();
		showToast('success', m.toast_saved());
	}

	async function onDeleteConnection(id: string) {
		if (!window.confirm('Delete this connection?')) return;
		await deleteConnection(id);
		await reload();
		showToast('success', m.toast_deleted());
	}

	async function onEditConnection(id: string) {
		wizardDraft = await getConnection(id);
		wizardOpen = true;
	}

	function envClass(env: string): string {
		if (env === 'production') return 'border-l-4 border-error';
		if (env === 'staging') return 'border-l-4 border-warning';
		if (env === 'development') return 'border-l-4 border-success';
		return 'border-l-4 border-base-300';
	}

	const grouped = $derived.by(() => {
		const map: Record<string, ConnectionSummary[]> = {};
		for (const c of connections) {
			if (filter && !c.name.toLowerCase().includes(filter.toLowerCase())) continue;
			const list = map[c.groupName] ?? [];
			list.push(c);
			map[c.groupName] = list;
		}
		return Object.entries(map);
	});

	async function onTreeActivate(connectionId: string, node: TreeNode) {
		selectedConnectionId = connectionId;
		if (node.kind === 'table' || node.kind === 'collection') {
			const sessionId = sessions[connectionId];
			if (!sessionId) return;
			const schema = typeof node.meta.schema === 'string' ? node.meta.schema : undefined;
			const id = `data-${connectionId}-${node.name}`;
			openSpecial({
				id,
				kind: 'data',
				title: node.name,
				connectionId,
				sessionId,
				schema,
				table: node.name
			});
			dataRows = await tablePage({
				sessionId,
				schema,
				table: node.name,
				offset: 0,
				limit: settings?.defaultMaxRows ?? 200,
				filterSql: dataFilter || null
			});
			const [cols] = await import('$lib/tool/dbms-ipc').then((mod) =>
				mod.describeTable(sessionId, schema ?? null, node.name)
			);
			inspectorText = cols.map((c) => `${c.name}: ${c.dataType}`).join('\n');
		}
	}

	async function openSelectedTable() {
		/* context-driven via tree */
	}

	async function openErd() {
		const connectionId = selectedConnectionId;
		if (!connectionId || !sessions[connectionId]) {
			showToast('error', 'Connect first.');
			return;
		}
		const sessionId = sessions[connectionId];
		erd = await getErd(sessionId, null);
		openSpecial({ id: `erd-${connectionId}`, kind: 'erd', title: m.erd_title(), sessionId });
	}

	async function doGenerateDdl() {
		const connectionId = selectedConnectionId;
		if (!connectionId || !sessions[connectionId]) return;
		const tree = trees[connectionId];
		const table = tree?.children
			.flatMap((c) => (c.kind === 'schema' ? c.children : [c]))
			.find((n) => n.kind === 'table' || n.kind === 'collection');
		if (!table) return;
		const schema = typeof table.meta.schema === 'string' ? table.meta.schema : null;
		const sql = await generateDdl(sessions[connectionId], schema, table.name);
		openSpecial({ id: `ddl-${table.name}`, kind: 'ddl', title: `DDL ${table.name}`, sql });
	}

	async function doExport(format: string) {
		if (!results) return;
		const text = await exportResults({
			sessionId: 'export',
			format,
			columns: results.columns,
			rows: results.rows
		});
		downloadText(`results.${format}`, text);
		showToast('success', m.toast_exported());
	}

	function downloadText(name: string, text: string) {
		const blob = new Blob([text], { type: 'text/plain' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = name;
		a.click();
		URL.revokeObjectURL(url);
	}

	async function doExportPack() {
		const passphrase = window.prompt('Passphrase (min 8 characters)');
		if (!passphrase) return;
		const pack = await exportConnectionPack(
			connections.map((c) => c.id),
			passphrase
		);
		downloadText('connections.pack.json', JSON.stringify(pack, null, 2));
		showToast('success', m.toast_exported());
	}

	async function doImportPack() {
		const raw = window.prompt('Paste connection pack JSON');
		if (!raw) return;
		const passphrase = window.prompt('Passphrase') ?? '';
		try {
			const packData = JSON.parse(raw);
			await importConnectionPack(packData, passphrase);
			await reload();
			showToast('success', m.toast_saved());
		} catch (err) {
			showToast('error', parseIpcError(err).message);
		}
	}

	async function saveAppSettings() {
		if (!settings) return;
		await saveSettings(settings);
		showToast('success', m.toast_settings());
	}

	async function finishOnboarding() {
		if (!settings) settings = await getSettings();
		settings = { ...settings, onboardingComplete: true };
		await saveSettings(settings);
		onboardingOpen = false;
	}

	async function runCompare() {
		const ids = Object.values(sessions);
		if (ids.length < 2) {
			showToast('error', 'Open two sessions to compare.');
			return;
		}
		compareOut = await compareSchemas({
			leftSessionId: ids[0]!,
			rightSessionId: ids[1]!
		});
	}

	async function applyPending() {
		const tab = activeTab;
		if (!tab || tab.kind !== 'data' || pendingEdits.length === 0) return;
		busy = true;
		try {
			await applyEdits({ sessionId: tab.sessionId, edits: pendingEdits });
			pendingEdits = [];
			dataRows = await tablePage({
				sessionId: tab.sessionId,
				schema: tab.schema,
				table: tab.table,
				offset: 0,
				limit: settings?.defaultMaxRows ?? 200,
				filterSql: dataFilter || null
			});
			showToast('success', m.toast_edits());
		} catch (err) {
			showToast('error', parseIpcError(err).message);
		} finally {
			busy = false;
		}
	}

	function cellEdit(rowIndex: number, col: string, value: string) {
		const tab = activeTab;
		if (!tab || tab.kind !== 'data' || !dataRows) return;
		const row = dataRows.rows[rowIndex];
		if (!row) return;
		const pk: Record<string, unknown> = {};
		const idIdx = dataRows.columns.indexOf('id');
		const oidIdx = dataRows.columns.indexOf('_id');
		if (idIdx >= 0) pk.id = row[idIdx];
		if (oidIdx >= 0) pk._id = row[oidIdx];
		if (Object.keys(pk).length === 0) {
			dataRows.columns.forEach((c, i) => {
				pk[c] = row[i];
			});
		}
		const colIdx = dataRows.columns.indexOf(col);
		if (colIdx >= 0) row[colIdx] = value;
		dataRows = { ...dataRows, rows: [...dataRows.rows] };
		pendingEdits = [
			...pendingEdits.filter((e) => !(e.table === tab.table && e.column === col && e.pk === pk)),
			{ schema: tab.schema, table: tab.table, pk, column: col, newValue: value }
		];
	}
</script>

<div
	class="rounded-box border-base-300 bg-base-100/80 flex h-full min-h-0 w-full flex-1 overflow-hidden border"
>
	{#if showNav}
		<aside class="border-base-300 bg-base-200/40 flex w-64 shrink-0 flex-col border-e md:w-72">
			<div class="border-base-300 flex items-center gap-2 border-b p-2">
				<span class="font-display text-sm font-semibold">{m.nav_title()}</span>
				<button
					type="button"
					class="btn btn-ghost btn-square btn-xs ms-auto cursor-pointer"
					aria-label={m.nav_add()}
					onclick={() => {
						wizardDraft = null;
						wizardOpen = true;
					}}
				>
					<WashIcon icon={washIcons.plus} class="size-4" />
				</button>
			</div>
			<input
				class="input input-sm input-bordered m-2 cursor-text"
				placeholder={m.nav_search()}
				bind:value={filter}
			/>
			<div class="min-h-0 flex-1 overflow-auto p-2">
				{#if connections.length === 0}
					<p class="text-sm opacity-70">{m.nav_empty()}</p>
				{:else}
					{#each grouped as [group, items] (group)}
						<p class="mt-2 mb-1 text-xs font-semibold tracking-wide uppercase opacity-60">
							{group}
						</p>
						{#each items as conn (conn.id)}
							<div class="rounded-box bg-base-100 mb-1 {envClass(conn.envColor)}">
								<div class="flex items-center gap-1 px-2 py-1">
									<WashIcon icon={washIcons.database} class="size-3.5 shrink-0" />
									<button
										type="button"
										class="min-w-0 flex-1 cursor-pointer truncate text-start text-sm"
										onclick={() => (selectedConnectionId = conn.id)}
									>
										{conn.name}
										<span class="ms-1 text-[0.65rem] opacity-50">{conn.engine}</span>
									</button>
									<button
										type="button"
										class="btn btn-ghost btn-square btn-xs cursor-pointer"
										aria-label={m.nav_connect()}
										onclick={() => void connect(conn.id)}
									>
										<WashIcon icon={washIcons.plug} class="size-3.5" />
									</button>
									<button
										type="button"
										class="btn btn-ghost btn-square btn-xs cursor-pointer"
										aria-label={m.nav_edit()}
										onclick={() => void onEditConnection(conn.id)}
									>
										<WashIcon icon={washIcons.pencil} class="size-3.5" />
									</button>
									<button
										type="button"
										class="btn btn-ghost btn-square btn-xs cursor-pointer"
										aria-label={m.nav_delete()}
										onclick={() => void onDeleteConnection(conn.id)}
									>
										<WashIcon icon={washIcons['trash-2']} class="size-3.5" />
									</button>
								</div>
								{#if trees[conn.id]}
									{@const tree = trees[conn.id]}
									<ul class="ps-4 pb-2 text-xs">
										{#each tree?.children ?? [] as node (node.id)}
											<li>
												{#if node.children.length}
													<details>
														<summary class="cursor-pointer py-0.5">{node.name}</summary>
														<ul class="ps-3">
															{#each node.children as child (child.id)}
																<li>
																	<button
																		type="button"
																		class="hover:text-primary cursor-pointer py-0.5"
																		onclick={() => void onTreeActivate(conn.id, child)}
																	>
																		{child.name}
																	</button>
																</li>
															{/each}
														</ul>
													</details>
												{:else}
													<button
														type="button"
														class="hover:text-primary cursor-pointer py-0.5"
														onclick={() => void onTreeActivate(conn.id, node)}
													>
														{node.name}
													</button>
												{/if}
											</li>
										{/each}
									</ul>
								{/if}
							</div>
						{/each}
					{/each}
				{/if}
			</div>
		</aside>
	{/if}

	<section class="flex min-w-0 flex-1 flex-col overflow-hidden">
		<div
			class="border-base-300 bg-base-200/50 flex flex-wrap items-center gap-1 border-b px-2 py-1"
		>
			{#each tabs as tab (tab.id)}
				<button
					type="button"
					class="btn btn-xs cursor-pointer {tab.id === activeTabId ? 'btn-primary' : 'btn-ghost'}"
					onclick={() => (activeTabId = tab.id)}
				>
					{tab.title}
				</button>
			{/each}
			{#if activeTab?.kind === 'console'}
				<button
					type="button"
					class="btn btn-success btn-xs ms-auto cursor-pointer"
					class:loading={busy}
					disabled={busy}
					onclick={() => void executeActive(false, false)}
				>
					<WashIcon icon={washIcons['circle-play']} class="size-3.5" />
					{m.console_run()}
				</button>
				<button
					type="button"
					class="btn btn-ghost btn-xs cursor-pointer"
					disabled={busy}
					onclick={() => void executeActive(true, false)}>{m.console_explain()}</button
				>
			{/if}
		</div>

		<div class="flex min-h-0 flex-1 flex-col overflow-hidden">
			{#if activeTab?.kind === 'console'}
				<div class="flex min-h-0 flex-[3] flex-col p-2">
					<WashCodeEditor
						bind:value={consoleSql}
						language={selectedConnectionId &&
						connections.find((c) => c.id === selectedConnectionId)?.engine === 'mongodb'
							? 'json'
							: 'sql'}
						wrap={settings?.editorWrap ?? false}
						onRun={() => void executeActive(false, false)}
					/>
				</div>
				<div class="border-base-300 flex min-h-0 flex-[2] flex-col border-t">
					<div class="border-base-300 flex gap-1 border-b px-2 py-1">
						<button
							type="button"
							class="btn btn-xs cursor-pointer {bottomTab === 'results'
								? 'btn-secondary'
								: 'btn-ghost'}"
							onclick={() => (bottomTab = 'results')}>{m.results_title()}</button
						>
						<button
							type="button"
							class="btn btn-xs cursor-pointer {bottomTab === 'messages'
								? 'btn-secondary'
								: 'btn-ghost'}"
							onclick={() => (bottomTab = 'messages')}>{m.messages_title()}</button
						>
						<button
							type="button"
							class="btn btn-xs cursor-pointer {bottomTab === 'plan'
								? 'btn-secondary'
								: 'btn-ghost'}"
							onclick={() => (bottomTab = 'plan')}>{m.plan_title()}</button
						>
						<div class="ms-auto flex gap-1">
							<button
								type="button"
								class="btn btn-ghost btn-xs cursor-pointer"
								onclick={() => void doExport('csv')}>{m.results_export_csv()}</button
							>
							<button
								type="button"
								class="btn btn-ghost btn-xs cursor-pointer"
								onclick={() => void doExport('json')}>{m.results_export_json()}</button
							>
							<button
								type="button"
								class="btn btn-ghost btn-xs cursor-pointer"
								onclick={() => void doExport('sql')}>{m.results_export_sql()}</button
							>
						</div>
					</div>
					<div class="min-h-0 flex-1 overflow-auto p-2">
						{#if !results}
							<p class="text-sm opacity-70">{m.results_empty()}</p>
						{:else if bottomTab === 'messages'}
							<pre class="font-mono text-xs whitespace-pre-wrap">{results.messages.join('\n') ||
									'—'}</pre>
						{:else if bottomTab === 'plan'}
							<pre class="font-mono text-xs whitespace-pre-wrap">{results.explainText ||
									'No plan'}</pre>
						{:else}
							<table class="table-zebra table-xs table">
								<thead class="bg-base-200 sticky top-0">
									<tr>
										{#each results.columns as col (col)}
											<th>{col}</th>
										{/each}
									</tr>
								</thead>
								<tbody>
									{#each results.rows as row, ri (ri)}
										<tr>
											{#each row as cell, ci (ci)}
												<td class="max-w-xs truncate font-mono">{String(cell ?? '')}</td>
											{/each}
										</tr>
									{/each}
								</tbody>
							</table>
							<p class="mt-1 text-xs opacity-60">
								{results.rowCount} row(s), {results.durationMs} ms{results.truncated
									? ' (truncated)'
									: ''}
							</p>
						{/if}
					</div>
				</div>
			{:else if activeTab?.kind === 'data'}
				<div class="border-base-300 flex flex-wrap items-center gap-2 border-b p-2">
					<input
						class="input input-sm input-bordered cursor-text"
						placeholder={m.data_filter()}
						bind:value={dataFilter}
					/>
					<button
						type="button"
						class="btn btn-sm btn-secondary cursor-pointer"
						onclick={async () => {
							dataRows = await tablePage({
								sessionId: activeTab.sessionId,
								schema: activeTab.schema,
								table: activeTab.table,
								offset: 0,
								limit: settings?.defaultMaxRows ?? 200,
								filterSql: dataFilter || null
							});
						}}>{m.refresh()}</button
					>
					<button
						type="button"
						class="btn btn-sm btn-primary cursor-pointer"
						class:loading={busy}
						disabled={busy || pendingEdits.length === 0}
						onclick={() => void applyPending()}>{m.data_apply_edits()}</button
					>
					<button
						type="button"
						class="btn btn-sm btn-ghost cursor-pointer"
						onclick={() => (pendingEdits = [])}>{m.data_discard()}</button
					>
				</div>
				<div class="min-h-0 flex-1 overflow-auto p-2">
					{#if dataRows}
						<table class="table-zebra table-xs [&_tbody_tr]:hover:bg-primary/20 table">
							<thead class="bg-base-200 sticky top-0">
								<tr>
									{#each dataRows.columns as col (col)}
										<th>{col}</th>
									{/each}
								</tr>
							</thead>
							<tbody>
								{#each dataRows.rows as row, ri (ri)}
									<tr>
										{#each dataRows.columns as col, ci (col)}
											<td>
												<input
													class="input input-xs input-ghost w-full cursor-text font-mono"
													value={String(row[ci] ?? '')}
													onchange={(e) =>
														cellEdit(ri, col, (e.currentTarget as HTMLInputElement).value)}
												/>
											</td>
										{/each}
									</tr>
								{/each}
							</tbody>
						</table>
					{/if}
				</div>
			{:else if activeTab?.kind === 'erd'}
				<div class="min-h-0 flex-1 overflow-auto p-4">
					{#if erd && erd.nodes.length}
						<svg viewBox="0 0 800 500" class="rounded-box bg-base-200 h-full w-full">
							{#each erd.edges as edge, i (i)}
								{@const fromIdx = erd.nodes.findIndex((n) => n.id === edge.from)}
								{@const toIdx = erd.nodes.findIndex((n) => n.id === edge.to)}
								{#if fromIdx >= 0 && toIdx >= 0}
									<line
										x1={80 + (fromIdx % 4) * 180}
										y1={60 + Math.floor(fromIdx / 4) * 140}
										x2={80 + (toIdx % 4) * 180}
										y2={60 + Math.floor(toIdx / 4) * 140}
										stroke="currentColor"
										class="opacity-40"
									/>
								{/if}
							{/each}
							{#each erd.nodes as node, i (node.id)}
								{@const x = 40 + (i % 4) * 180}
								{@const y = 30 + Math.floor(i / 4) * 140}
								<rect
									{x}
									{y}
									width="140"
									height="90"
									rx="8"
									class="fill-base-100 stroke-base-content/30"
								/>
								<text {x} y={y + 18} class="fill-base-content text-[11px] font-semibold" dx="8"
									>{node.label}</text
								>
								{#each node.columns.slice(0, 4) as col, ci (col)}
									<text {x} y={y + 36 + ci * 12} class="fill-base-content/70 text-[9px]" dx="8"
										>{col}</text
									>
								{/each}
							{/each}
						</svg>
					{:else}
						<p class="opacity-70">{m.erd_empty()}</p>
					{/if}
				</div>
			{:else if activeTab?.kind === 'compare'}
				<div class="space-y-3 overflow-auto p-4">
					<button
						type="button"
						class="btn btn-primary cursor-pointer"
						onclick={() => void runCompare()}>{m.compare_run()}</button
					>
					{#if compareOut}
						<pre class="font-mono text-xs whitespace-pre-wrap">{JSON.stringify(
								{
									onlyLeft: compareOut.onlyLeft,
									onlyRight: compareOut.onlyRight,
									differing: compareOut.differing
								},
								null,
								2
							)}</pre>
						<pre class="font-mono text-xs whitespace-pre-wrap">{compareOut.migrationSql}</pre>
					{/if}
				</div>
			{:else if activeTab?.kind === 'settings' && settings}
				<div class="space-y-3 overflow-auto p-4">
					<label class="flex cursor-pointer items-center gap-2">
						<input type="checkbox" class="checkbox" bind:checked={settings.confirmDestructive} />
						{m.settings_confirm_destructive()}
					</label>
					<label class="form-control max-w-xs">
						<span class="label-text">{m.settings_max_rows()}</span>
						<input
							type="number"
							class="input input-bordered cursor-text"
							bind:value={settings.defaultMaxRows}
						/>
					</label>
					<label class="flex cursor-pointer items-center gap-2">
						<input type="checkbox" class="checkbox" bind:checked={settings.editorWrap} />
						{m.settings_wrap()}
					</label>
					<label class="flex cursor-pointer items-center gap-2">
						<input type="checkbox" class="checkbox" bind:checked={settings.autoCommit} />
						{m.settings_auto_commit()}
					</label>
					<button
						type="button"
						class="btn btn-primary cursor-pointer"
						onclick={() => void saveAppSettings()}>{m.settings_save()}</button
					>
				</div>
			{:else if activeTab?.kind === 'history'}
				<div class="min-h-0 flex-1 overflow-auto p-2">
					{#if history.length === 0}
						<p class="opacity-70">{m.history_empty()}</p>
					{:else}
						<ul class="space-y-2">
							{#each history as item (item.id)}
								<li class="rounded-box border-base-300 border p-2">
									<pre class="font-mono text-xs whitespace-pre-wrap">{item.sqlText}</pre>
									<p class="text-[0.65rem] opacity-60">
										{item.status} · {item.durationMs ?? 0} ms · {item.createdAt}
									</p>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			{:else if activeTab?.kind === 'ddl'}
				<div class="min-h-0 flex-1 p-2">
					<pre
						class="rounded-box bg-base-200 h-full overflow-auto p-3 font-mono text-sm whitespace-pre-wrap">{activeTab.sql}</pre>
				</div>
			{:else}
				<p class="p-4 opacity-70">{m.console_empty()}</p>
			{/if}
		</div>
	</section>

	{#if showInspector}
		<aside
			class="border-base-300 bg-base-200/30 hidden w-56 shrink-0 flex-col border-s p-2 lg:flex"
		>
			<p class="font-display mb-2 text-sm font-semibold">{m.inspector_title()}</p>
			{#if inspectorText}
				<pre
					class="min-h-0 flex-1 overflow-auto font-mono text-[0.7rem] whitespace-pre-wrap">{inspectorText}</pre>
			{:else}
				<p class="text-xs opacity-60">{m.inspector_empty()}</p>
			{/if}
			{#if selectedConnectionId}
				<button
					type="button"
					class="btn btn-ghost btn-xs mt-2 cursor-pointer"
					onclick={() =>
						void addBookmark(
							selectedConnectionId!,
							'root',
							connections.find((c) => c.id === selectedConnectionId)?.name ?? 'bookmark'
						)}>{m.nav_bookmark()}</button
				>
			{/if}
		</aside>
	{/if}
</div>

<ConnectionWizard
	open={wizardOpen}
	initial={wizardDraft}
	onSave={onSaveConnection}
	onClose={() => (wizardOpen = false)}
/>

{#if onboardingOpen}
	<div class="bg-base-300/70 fixed inset-0 z-50 flex items-center justify-center p-4">
		<div class="rounded-box border-base-300 bg-base-100 max-w-lg border p-6 shadow-lg">
			<h2 class="font-display text-primary mb-2 text-xl font-bold">{m.onboarding_title()}</h2>
			<p class="mb-4 text-sm opacity-80">{m.onboarding_body()}</p>
			<button
				type="button"
				class="btn btn-primary cursor-pointer"
				onclick={() => void finishOnboarding()}>{m.onboarding_done()}</button
			>
		</div>
	</div>
{/if}

{#if aboutOpen}
	<div class="bg-base-300/70 fixed inset-0 z-50 flex items-center justify-center p-4">
		<div class="rounded-box border-base-300 bg-base-100 max-w-md border p-6 shadow-lg">
			<h2 class="font-display text-primary mb-2 text-xl font-bold">{m.app_title()}</h2>
			<p class="mb-4 text-sm opacity-80">{m.about_body()}</p>
			<button type="button" class="btn cursor-pointer" onclick={() => (aboutOpen = false)}
				>OK</button
			>
		</div>
	</div>
{/if}

{#if toast}
	<div class="toast toast-bottom toast-end z-[100]">
		<div class="alert alert-{toast.tone} shadow-lg">
			<span>{toast.text}</span>
		</div>
	</div>
{/if}
