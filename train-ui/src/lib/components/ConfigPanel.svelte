<script lang="ts">
	import Button from '$lib/components/Button.svelte';
</script>

<div class="config-panel">
	<label for="gtfs">Load GTFS:</label>
	<input type="file" accept=".zip" id="gtfs" class="cfg-style">

	<label for="model-date">Date to Model:</label>
	<input type="date" id="model-date" value="2024-05-10" class="cfg-style">

	<div class="network-buttons">
		<Button text="Save Network from Disk" command="save_network" class="cfg-style" />
		<Button text="Load Network from Disk" command="load_network" class="cfg-style" />
	</div>

	<Button text="Patronage Data Import" command="data_import" class="cfg-style" />

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

	.network-buttons {
		display: flex;
		width: 100%;
		gap: 10px;
	}
</style>
