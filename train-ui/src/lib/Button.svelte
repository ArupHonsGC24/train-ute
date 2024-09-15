<script lang="ts">
  export let text: string;
  export let disabledTooltip = "";
  export let disabled = false;

  let className = "";
  // noinspection JSUnusedGlobalSymbols, ReservedWordAsName
  export { className as class };

  // If the button is disabled, we want to show the tooltip.
  $: title = disabled ? disabledTooltip : "";
</script>

<button type="button" {title} class={className} {disabled} on:click>{text}</button>

<style>
  button {
    background-color: #a28a6f;
    color: white;
    cursor: inherit;
  }

  button:disabled {
    background-color: #5e503f;
    color: #a28a6f;
  }

  button:hover:enabled {
    background-color: #5e503f;
  }
</style>
