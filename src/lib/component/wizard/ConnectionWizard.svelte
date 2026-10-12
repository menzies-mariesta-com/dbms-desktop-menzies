<script lang="ts">
	import { m } from '$lib/paraglide/messages.js';
	import { emptyDraft, type ConnectionDraft, type Engine } from '$lib/schemas/dbms';
	import { testConnection } from '$lib/tool/dbms-ipc';
	import { open } from '@tauri-apps/plugin-dialog';

	type Props = {
		open?: boolean;
		initial?: ConnectionDraft | null;
		onSave: (draft: ConnectionDraft) => Promise<void>;
		onClose: () => void;
	};

	let { open: isOpen = false, initial = null, onSave, onClose }: Props = $props();

	let draft = $state<ConnectionDraft>(emptyDraft());
	let busy = $state(false);
	let testing = $state(false);
	let message = $state('');

	$effect(() => {
		if (isOpen) {
			draft = initial ? structuredClone(initial) : emptyDraft('sqlite');
			message = '';
		}
	});

	const engines: Engine[] = ['postgresql', 'mysql', 'mariadb', 'mssql', 'sqlite', 'mongodb'];

	async function browseSqlite() {
		try {
			const selected = await open({
				multiple: false,
				filters: [{ name: 'SQLite', extensions: ['db', 'sqlite', 'sqlite3'] }]
			});
			if (typeof selected === 'string') draft.filePath = selected;
		} catch {
			/* browser */
		}
	}

	async function onTest() {
		testing = true;
		message = '';
		try {
			message = await testConnection(draft);
		} catch (err) {
			message = err instanceof Error ? err.message : String(err);
		} finally {
			testing = false;
		}
	}

	async function onSubmit(event: Event) {
		event.preventDefault();
		busy = true;
		message = '';
		try {
			await onSave(draft);
			onClose();
		} catch (err) {
			message = err instanceof Error ? err.message : String(err);
		} finally {
			busy = false;
		}
	}
</script>

{#if isOpen}
	<div class="bg-base-300/60 fixed inset-0 z-50 flex items-center justify-center p-4">
		<form
			class="rounded-box border-base-300 bg-base-100 flex max-h-[90dvh] w-full max-w-2xl flex-col overflow-hidden border shadow-lg"
			onsubmit={onSubmit}
		>
			<header class="border-base-300 border-b px-4 py-3">
				<h2 class="font-display text-primary text-lg font-bold">
					{initial ? m.wizard_title_edit() : m.wizard_title_create()}
				</h2>
			</header>
			<div class="min-h-0 flex-1 space-y-3 overflow-y-auto p-4">
				<label class="form-control">
					<span class="label-text"
						>{m.wizard_name()}<span class="text-error align-top text-sm" aria-hidden="true">*</span
						></span
					>
					<input class="input input-bordered cursor-text" required bind:value={draft.name} />
				</label>
				<div class="grid gap-3 sm:grid-cols-3">
					<label class="form-control">
						<span class="label-text">{m.wizard_engine()}</span>
						<select class="select select-bordered cursor-pointer" bind:value={draft.engine}>
							{#each engines as eng (eng)}
								<option value={eng}>{eng}</option>
							{/each}
						</select>
					</label>
					<label class="form-control">
						<span class="label-text">{m.wizard_group()}</span>
						<input class="input input-bordered cursor-text" bind:value={draft.groupName} />
					</label>
					<label class="form-control">
						<span class="label-text">{m.wizard_env()}</span>
						<select class="select select-bordered cursor-pointer" bind:value={draft.envColor}>
							<option value="production">{m.env_production()}</option>
							<option value="staging">{m.env_staging()}</option>
							<option value="development">{m.env_development()}</option>
							<option value="neutral">{m.env_neutral()}</option>
						</select>
					</label>
				</div>

				{#if draft.engine === 'sqlite'}
					<label class="form-control">
						<span class="label-text"
							>{m.wizard_file()}<span class="text-error align-top text-sm" aria-hidden="true"
								>*</span
							></span
						>
						<div class="flex gap-2">
							<input
								class="input input-bordered min-w-0 flex-1 cursor-text"
								required
								bind:value={draft.filePath}
							/>
							<button
								type="button"
								class="btn btn-ghost cursor-pointer"
								onclick={() => void browseSqlite()}>{m.wizard_browse()}</button
							>
						</div>
					</label>
				{:else}
					<div class="grid gap-3 sm:grid-cols-2">
						<label class="form-control">
							<span class="label-text">{m.wizard_host()}</span>
							<input class="input input-bordered cursor-text" bind:value={draft.host} />
						</label>
						<label class="form-control">
							<span class="label-text">{m.wizard_port()}</span>
							<input
								class="input input-bordered cursor-text"
								type="number"
								bind:value={draft.port}
							/>
						</label>
						<label class="form-control">
							<span class="label-text">{m.wizard_database()}</span>
							<input class="input input-bordered cursor-text" bind:value={draft.database} />
						</label>
						<label class="form-control">
							<span class="label-text">{m.wizard_username()}</span>
							<input class="input input-bordered cursor-text" bind:value={draft.username} />
						</label>
						<label class="form-control sm:col-span-2">
							<span class="label-text">{m.wizard_password()}</span>
							<input
								class="input input-bordered cursor-text"
								type="password"
								bind:value={draft.password}
							/>
						</label>
						{#if draft.engine === 'mongodb'}
							<label class="form-control sm:col-span-2">
								<span class="label-text">{m.wizard_uri()}</span>
								<input class="input input-bordered cursor-text" bind:value={draft.connectionUri} />
							</label>
						{/if}
					</div>
					<label class="form-control">
						<span class="label-text">{m.wizard_ssl_mode()}</span>
						<select class="select select-bordered cursor-pointer" bind:value={draft.ssl.mode}>
							<option value="disable">disable</option>
							<option value="prefer">prefer</option>
							<option value="require">require</option>
						</select>
					</label>
					<label class="flex cursor-pointer items-center gap-2">
						<input type="checkbox" class="checkbox" bind:checked={draft.ssh.enabled} />
						<span>{m.wizard_ssh_enabled()}</span>
					</label>
					{#if draft.ssh.enabled}
						<div class="grid gap-3 sm:grid-cols-2">
							<label class="form-control">
								<span class="label-text">{m.wizard_ssh_host()}</span>
								<input class="input input-bordered cursor-text" bind:value={draft.ssh.host} />
							</label>
							<label class="form-control">
								<span class="label-text">{m.wizard_ssh_port()}</span>
								<input
									class="input input-bordered cursor-text"
									type="number"
									bind:value={draft.ssh.port}
								/>
							</label>
							<label class="form-control">
								<span class="label-text">{m.wizard_ssh_user()}</span>
								<input class="input input-bordered cursor-text" bind:value={draft.ssh.username} />
							</label>
							<label class="form-control">
								<span class="label-text">{m.wizard_ssh_auth()}</span>
								<select
									class="select select-bordered cursor-pointer"
									bind:value={draft.ssh.authMethod}
								>
									<option value="password">password</option>
									<option value="private_key">private_key</option>
								</select>
							</label>
							{#if draft.ssh.authMethod === 'private_key'}
								<label class="form-control sm:col-span-2">
									<span class="label-text">{m.wizard_ssh_key()}</span>
									<input
										class="input input-bordered cursor-text"
										bind:value={draft.ssh.privateKeyPath}
									/>
								</label>
							{/if}
						</div>
					{/if}
				{/if}

				{#if message}
					<p class="rounded-box bg-base-200 p-2 font-mono text-xs whitespace-pre-wrap">{message}</p>
				{/if}
			</div>
			<footer class="border-base-300 flex flex-wrap justify-end gap-2 border-t px-4 py-3">
				<button type="button" class="btn btn-ghost cursor-pointer" onclick={onClose}
					>{m.wizard_cancel()}</button
				>
				<button
					type="button"
					class="btn btn-secondary cursor-pointer"
					class:loading={testing}
					disabled={testing || busy}
					onclick={() => void onTest()}>{m.wizard_test()}</button
				>
				<button
					type="submit"
					class="btn btn-primary cursor-pointer"
					class:loading={busy}
					disabled={busy || testing}>{m.wizard_save()}</button
				>
			</footer>
		</form>
	</div>
{/if}
