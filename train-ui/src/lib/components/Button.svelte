<script lang="ts">
  import { invoke, type InvokeArgs } from "@tauri-apps/api/core";

  export let text: string;
  export let disabled_tooltip = "";
  export let command: string;
  export let args: InvokeArgs | undefined = undefined;
  export let headers: Record<string, string> | undefined = undefined;
  export let disabled = false;

  let className = "";
  // noinspection JSUnusedGlobalSymbols,ReservedWordAsName
  export { className as class };

  // If the button is disabled, we want to show the tooltip.
  $: title = disabled ? disabled_tooltip : "";

  async function handleClick() {
    console.log(`Invoking ${command} with args: ${args}`);
    await invoke(`${command}`, args, headers ? { headers } : undefined);
  }
</script>

<button
  type="button"
  {title}
  class={className}
  {disabled}
  on:click={handleClick}>{text}</button
>

<style>
  button {
    background-color: #a28a6f;
    color: white;
    cursor: pointer;
  }

  button:disabled {
    background-color: #5e503f;
    color: #a28a6f;
    cursor: not-allowed;
  }

  button:hover:enabled {
    background-color: #5e503f;
  }
</style>
