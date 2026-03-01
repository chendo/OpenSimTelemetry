import { execSync } from 'child_process';
import path from 'path';

export default function globalSetup() {
  if (process.env.API_BASE_URL) {
    console.log('Using external server, skipping build.');
    return;
  }
  const root = path.resolve(__dirname, '../..');
  console.log('Building ost-server (release)...');
  execSync('cargo build --release -p ost-server', {
    cwd: root,
    stdio: 'inherit',
    timeout: 300_000,
  });
  console.log('Build complete.');
}
