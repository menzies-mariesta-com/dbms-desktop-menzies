import { invoke } from '@tauri-apps/api/core';
import type {
	AppSettings,
	ConnectionDraft,
	ConnectionSummary,
	QueryResult,
	TreeNode,
	ColumnInfo
} from '$lib/schemas/dbms';

function isTauri(): boolean {
	return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

const browserConnections: ConnectionSummary[] = [];
const browserSettings: AppSettings = {
	confirmDestructive: true,
	defaultMaxRows: 500,
	editorWrap: false,
	autoCommit: true,
	onboardingComplete: false
};

export async function listConnections(): Promise<ConnectionSummary[]> {
	if (!isTauri()) return browserConnections;
	return invoke('list_connections');
}

export async function saveConnection(draft: ConnectionDraft): Promise<ConnectionSummary> {
	if (!isTauri()) {
		const summary: ConnectionSummary = {
			id: draft.id ?? crypto.randomUUID(),
			name: draft.name,
			engine: draft.engine,
			groupName: draft.groupName,
			envColor: draft.envColor,
			host: draft.host,
			port: draft.port,
			database: draft.database,
			username: draft.username,
			filePath: draft.filePath,
			hasPassword: Boolean(draft.password),
			sslMode: draft.ssl.mode,
			sshEnabled: draft.ssh.enabled,
			createdAt: new Date().toISOString(),
			updatedAt: new Date().toISOString()
		};
		const idx = browserConnections.findIndex((c) => c.id === summary.id);
		if (idx >= 0) browserConnections[idx] = summary;
		else browserConnections.push(summary);
		return summary;
	}
	return invoke('save_connection', { draft });
}

export async function deleteConnection(id: string): Promise<void> {
	if (!isTauri()) {
		const idx = browserConnections.findIndex((c) => c.id === id);
		if (idx >= 0) browserConnections.splice(idx, 1);
		return;
	}
	return invoke('delete_connection', { id });
}

export async function getConnection(id: string): Promise<ConnectionDraft> {
	if (!isTauri()) throw new Error('Desktop required');
	return invoke('get_connection', { id });
}

export async function testConnection(draft: ConnectionDraft): Promise<string> {
	if (!isTauri()) return 'Browser preview: connection test skipped.';
	return invoke('test_connection', { draft });
}

export async function openSession(connectionId: string): Promise<string> {
	if (!isTauri()) return `browser-${connectionId}`;
	return invoke('open_session', { connectionId });
}

export async function closeSession(sessionId: string): Promise<void> {
	if (!isTauri()) return;
	return invoke('close_session', { sessionId });
}

export async function listTree(sessionId: string): Promise<TreeNode> {
	if (!isTauri()) {
		return {
			id: 'demo',
			kind: 'database',
			name: 'Preview',
			engine: 'sqlite',
			children: [],
			meta: {}
		};
	}
	return invoke('list_tree', { sessionId });
}

export async function describeTable(
	sessionId: string,
	schema: string | null,
	table: string
): Promise<[ColumnInfo[], unknown[]]> {
	if (!isTauri()) return [[], []];
	return invoke('describe_table', { sessionId, schema, table });
}

export async function runQuery(
	request: {
		sessionId: string;
		sql: string;
		maxRows?: number;
		explain: boolean;
		analyze: boolean;
	},
	connectionId?: string,
	confirmDestructive = false
): Promise<QueryResult> {
	if (!isTauri()) {
		return {
			columns: ['preview'],
			rows: [['Run inside the desktop app for live results']],
			rowCount: 1,
			truncated: false,
			durationMs: 0,
			messages: [],
			explainText: null
		};
	}
	return invoke('run_query', { request, connectionId: connectionId ?? null, confirmDestructive });
}

export async function tablePage(request: {
	sessionId: string;
	schema?: string | null;
	table: string;
	offset: number;
	limit: number;
	filterSql?: string | null;
	orderBy?: string | null;
}): Promise<QueryResult> {
	if (!isTauri()) {
		return {
			columns: [],
			rows: [],
			rowCount: 0,
			truncated: false,
			durationMs: 0,
			messages: ['Desktop required'],
			explainText: null
		};
	}
	return invoke('table_page', { request });
}

export async function applyEdits(request: {
	sessionId: string;
	edits: Array<{
		schema?: string | null;
		table: string;
		pk: Record<string, unknown>;
		column: string;
		newValue: unknown;
	}>;
}): Promise<number> {
	if (!isTauri()) return 0;
	return invoke('apply_edits', { request });
}

export async function getErd(
	sessionId: string,
	schema?: string | null
): Promise<{
	nodes: Array<{ id: string; label: string; columns: string[] }>;
	edges: Array<{ from: string; to: string; label: string }>;
}> {
	if (!isTauri()) return { nodes: [], edges: [] };
	return invoke('get_erd', { sessionId, schema: schema ?? null });
}

export async function generateDdl(
	sessionId: string,
	schema: string | null,
	table: string
): Promise<string> {
	if (!isTauri()) return '-- desktop required';
	return invoke('generate_ddl', { sessionId, schema, table });
}

export async function compareSchemas(request: {
	leftSessionId: string;
	rightSessionId: string;
	leftSchema?: string | null;
	rightSchema?: string | null;
}): Promise<{
	onlyLeft: string[];
	onlyRight: string[];
	differing: string[];
	migrationSql: string;
}> {
	if (!isTauri()) {
		return { onlyLeft: [], onlyRight: [], differing: [], migrationSql: '' };
	}
	return invoke('compare_schemas', { request });
}

export async function listHistory(limit = 100): Promise<
	Array<{
		id: string;
		connectionId?: string | null;
		engine?: string | null;
		sqlText: string;
		status: string;
		durationMs?: number | null;
		createdAt: string;
	}>
> {
	if (!isTauri()) return [];
	return invoke('list_history', { limit });
}

export async function getSettings(): Promise<AppSettings> {
	if (!isTauri()) return { ...browserSettings };
	return invoke('get_settings');
}

export async function saveSettings(settings: AppSettings): Promise<void> {
	if (!isTauri()) {
		Object.assign(browserSettings, settings);
		return;
	}
	return invoke('save_settings', { settings });
}

export async function exportResults(request: {
	sessionId: string;
	format: string;
	columns: string[];
	rows: unknown[][];
}): Promise<string> {
	if (!isTauri()) return JSON.stringify(request.rows, null, 2);
	return invoke('export_results', { request });
}

export async function exportBackup(): Promise<string> {
	if (!isTauri()) return '{}';
	return invoke('export_backup');
}

export async function exportConnectionPack(
	ids: string[],
	passphrase: string
): Promise<{ version: number; encrypted: boolean; payload: string }> {
	if (!isTauri()) throw new Error('Desktop required');
	return invoke('export_connection_pack', { ids, passphrase });
}

export async function importConnectionPack(
	packData: { version: number; encrypted: boolean; payload: string },
	passphrase: string
): Promise<ConnectionSummary[]> {
	if (!isTauri()) throw new Error('Desktop required');
	return invoke('import_connection_pack', { packData, passphrase });
}

export async function resultDiff(left: QueryResult, right: QueryResult): Promise<unknown> {
	if (!isTauri()) return { onlyLeft: [], onlyRight: [], sharedCount: 0 };
	return invoke('result_diff', { left, right });
}

export async function addBookmark(
	connectionId: string,
	objectPath: string,
	label: string
): Promise<unknown> {
	if (!isTauri()) return {};
	return invoke('add_bookmark', { connectionId, objectPath, label });
}

export function parseIpcError(err: unknown): { code: string; message: string } {
	const raw = typeof err === 'string' ? err : err instanceof Error ? err.message : String(err);
	try {
		const parsed = JSON.parse(raw) as { code?: string; message?: string };
		return { code: parsed.code ?? 'ERROR', message: parsed.message ?? raw };
	} catch {
		return { code: 'ERROR', message: raw };
	}
}
