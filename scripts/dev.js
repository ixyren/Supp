// Simple script to run Vite dev server and Electron concurrently
const { spawn } = require('child_process');
const waitOn = require('wait-on');
const path = require('path');

console.log('Starting Vite dev server...');

// Start Vite dev server
const vite = spawn('node', [path.join(__dirname, '../node_modules/vite/bin/vite.js')], {
    stdio: 'inherit',
    cwd: path.join(__dirname, '..'),
    env: { ...process.env, NODE_ENV: 'development' }
});

vite.on('error', (err) => {
    console.error('Failed to start Vite:', err);
    process.exit(1);
});

// Wait for Vite to be ready, then start Electron
waitOn({
    resources: ['http://localhost:5173'],
    timeout: 60000,
    interval: 1000,
    verbose: true
}).then(() => {
    console.log('\n✓ Vite is ready! Starting Electron...\n');

    // Use require.resolve to get the correct electron path
    const electronPath = require('electron');
    const appPath = path.join(__dirname, '..');

    const electron = spawn(electronPath, [appPath], {
        stdio: 'inherit',
        cwd: appPath,
        env: { ...process.env, NODE_ENV: 'development' }
    });

    electron.on('error', (err) => {
        console.error('Failed to start Electron:', err);
        vite.kill();
        process.exit(1);
    });

    electron.on('close', (code) => {
        console.log(`\nElectron closed with code ${code}. Stopping Vite...`);
        vite.kill();
        process.exit(0);
    });
}).catch(err => {
    console.error('Error waiting for Vite:', err);
    vite.kill();
    process.exit(1);
});

// Handle Ctrl+C
process.on('SIGINT', () => {
    console.log('\nStopping all processes...');
    vite.kill();
    process.exit(0);
});

process.on('SIGTERM', () => {
    console.log('\nStopping all processes...');
    vite.kill();
    process.exit(0);
});
