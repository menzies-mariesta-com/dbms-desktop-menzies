export type DbmsMenuAction =
	| { type: 'new-console' }
	| { type: 'new-connection' }
	| { type: 'refresh-tree' }
	| { type: 'run' }
	| { type: 'run-script' }
	| { type: 'explain' }
	| { type: 'history' }
	| { type: 'settings' }
	| { type: 'erd' }
	| { type: 'schema-compare' }
	| { type: 'export-results' }
	| { type: 'export-backup' }
	| { type: 'export-pack' }
	| { type: 'import-pack' }
	| { type: 'toggle-navigator' }
	| { type: 'toggle-inspector' }
	| { type: 'onboarding' }
	| { type: 'about' }
	| { type: 'generate-ddl' }
	| { type: 'open-table' }
	| { type: 'result-diff' }
	| { type: 'disconnect' };

const listeners = new Set<(action: DbmsMenuAction) => void>();

export function requestDbmsMenuAction(action: DbmsMenuAction): void {
	for (const listener of listeners) listener(action);
}

export function subscribeDbmsMenu(listener: (action: DbmsMenuAction) => void): () => void {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}
