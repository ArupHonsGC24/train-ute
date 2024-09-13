<script lang="ts">
	import { invoke, type InvokeArgs } from '@tauri-apps/api/core';

	export let text: string;
	export let disabled_tooltip = '';
	export let command: string;
	export let args: InvokeArgs | undefined = undefined;
	export let disabled = false;

	let className = '';
	export { className as class };

	// If the button is disabled, we want to show the tooltip.
	$: title = disabled ? disabled_tooltip : '';

	async function handleClick() {
		console.log(`Invoking ${command} with args: ${args}`);
		await invoke(`${command}`, args);
	}
</script>

<button type="button" {title} class={className} {disabled} on:click={handleClick}>{text}</button>

<style>
	button {
		background-color: #A28A6F;
		color: white;
		cursor: pointer;
	}

	button:disabled {
		background-color: #5E503F;
		color: #A28A6F;
		cursor: not-allowed;
	}

	button:hover:enabled {
		background-color: #5E503F;
	}
</style>