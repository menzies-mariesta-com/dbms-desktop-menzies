const listeners = new Set<() => void>();

export function requestDbmsRefresh(): void {
	for (const listener of listeners) listener();
}

export function subscribeDbmsRefresh(listener: () => void): () => void {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}
