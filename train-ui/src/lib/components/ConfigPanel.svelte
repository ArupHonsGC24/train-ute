<script lang="ts">
	import Button from '$lib/components/Button.svelte';

	let input_files: FileList | null = null;
	let loaded_gtfs_zip: ArrayBuffer | undefined = undefined;

	// A bit of fancy async/await to handle the file upload.
	async function loadGTFS(event: Event) {
		if (input_files && input_files.length > 0) {
			let file = input_files[0];

			if (file.name.endsWith('.zip')) {
				loaded_gtfs_zip = await file.arrayBuffer();
			} else {
				alert('Invalid file type. Please load a .zip file.');
				input_files = null;
				loaded_gtfs_zip = undefined;
				(event.currentTarget as HTMLInputElement).value = '';
			}
		}
	}

	let model_date = '2024-05-10';
</script>

<div class="config-panel">
	<label for="gtfs">Load GTFS:</label>
	<input type="file" accept=".zip" id="gtfs" class="cfg-style" bind:files={input_files} on:change={loadGTFS}>

	<label for="model-date">Date to Model:</label>
	<input type="date" id="model-date" class="cfg-style" bind:value={model_date}>

	<!--
		<div class="network-buttons">
			<Button text="Save Network to Disk" command="save_network" class="cfg-style" />
			<Button text="Load Network from Disk" command="load_network" class="cfg-style" />
		</div>
	-->

	<Button text="Generate Network"
					class="cfg-style"
					command="gen_network"
					disabled={loaded_gtfs_zip == null}
					disabled_tooltip="Load GTFS and select date first."
					args={ loaded_gtfs_zip }
					headers={{ model_date }}
	/>

	<Button text="Patronage Data Import" command="print_hello" class="cfg-style" />

	<Button text="Run Simulation" command="run_simulation" class="cfg-style" />

	<Button text="Export Results" command="export" class="cfg-style" />
</div>

<style>
	label {
		font-size: 1.2rem;
		color: white;
	}

	.config-panel {
		display: flex;
		flex-direction: column;
		justify-content: start;
		align-items: flex-start;
		gap: 20px;
	}

	.config-panel :global(.cfg-style) {
		width: 100%;
		padding: 10px;
		font-size: 1rem;
		border-radius: 5px;
		border: none;
	}

	input {
		background-color: #5E503F;
		color: white;
	}

	input[type="file"] {
		color-scheme: light;
	}

	.network-buttons {
		display: flex;
		width: 100%;
		gap: 10px;
	}
</style>
