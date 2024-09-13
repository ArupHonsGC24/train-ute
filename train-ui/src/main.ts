import '../fonts/inter.css';
import './style.css';

import { invoke } from '@tauri-apps/api/core';

const buttons = document.getElementsByTagName('button');

for (let button of buttons) {
  button.addEventListener('click', async (event) => {
    let element = event.target as HTMLButtonElement;
    await invoke('my_custom_command', { button: element.name });
  });
}
