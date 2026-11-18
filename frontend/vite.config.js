import {defineConfig} from 'vite';

export default defineConfig({
  build: {
    modulePreload: false,
    rolldownOptions: {
      input: 'src/main.tsx',
      output: {entryFileNames: 'sever-ui.js', codeSplitting: false},
    },
  },
});
