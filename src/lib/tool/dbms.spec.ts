import { describe, expect, it } from 'vitest';
import { emptyDraft, connectionDraftSchema } from '$lib/schemas/dbms';
import { parseIpcError } from '$lib/tool/dbms-ipc';

describe('dbms schemas', () => {
	it('builds a valid sqlite draft', () => {
		const draft = emptyDraft('sqlite');
		draft.name = 'Local';
		draft.filePath = '/tmp/demo.db';
		const parsed = connectionDraftSchema.parse(draft);
		expect(parsed.engine).toBe('sqlite');
		expect(parsed.name).toBe('Local');
	});

	it('parses coded IPC errors', () => {
		const err = parseIpcError(
			JSON.stringify({ code: 'DESTRUCTIVE_CONFIRM', message: 'Confirm first' })
		);
		expect(err.code).toBe('DESTRUCTIVE_CONFIRM');
		expect(err.message).toBe('Confirm first');
	});
});
