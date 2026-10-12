<script lang="ts">
	type Props = {
		value?: string;
		language?: 'sql' | 'json';
		fileName?: string;
		wrap?: boolean;
		onRun?: () => void;
	};

	let {
		value = $bindable(''),
		language = 'sql',
		fileName = 'query.sql',
		wrap = false,
		onRun
	}: Props = $props();

	const lines = $derived(value.length === 0 ? 1 : value.split('\n').length);

	function onKeydown(event: KeyboardEvent) {
		if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
			event.preventDefault();
			onRun?.();
		}
	}
</script>

<div
	class="wash-code-editor border-base-300 bg-base-100 rounded-box flex min-h-0 flex-1 flex-col overflow-hidden border"
>
	<div
		class="wash-code-titlebar border-base-300 bg-base-200 flex flex-wrap items-center gap-2 border-b px-3 py-1.5"
	>
		<div class="wash-code-dots flex gap-1" aria-hidden="true">
			<span class="bg-error/70 size-2 rounded-full"></span>
			<span class="bg-warning/70 size-2 rounded-full"></span>
			<span class="bg-success/70 size-2 rounded-full"></span>
		</div>
		<span class="font-mono text-xs opacity-70">{fileName}</span>
		<span class="ms-auto font-mono text-[0.65rem] uppercase opacity-50">{language}</span>
	</div>
	<div class="relative flex min-h-0 flex-1 overflow-hidden">
		<div
			class="border-base-300 bg-base-200/50 text-base-content/50 border-e px-2 py-3 text-right font-mono text-xs select-none"
			aria-hidden="true"
		>
			{#each Array.from({ length: lines }, (_, i) => i + 1) as n (n)}
				<div class="leading-relaxed">{n}</div>
			{/each}
		</div>
		<textarea
			class="bg-base-100 min-h-0 w-full flex-1 resize-none border-0 p-3 font-mono text-sm leading-relaxed outline-none {wrap
				? 'whitespace-pre-wrap'
				: 'whitespace-pre'} cursor-text"
			bind:value
			spellcheck="false"
			aria-label="Query editor"
			onkeydown={onKeydown}></textarea>
	</div>
</div>
