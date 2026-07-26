import { defineConfig } from 'vite'
import dns from 'node:dns/promises'

export default defineConfig(async () => {
    let config = {
        logLevel: 'info',
        build: {
            // generate .vite/manifest.json in outDir
            manifest: true,
            rolldownOptions: {
                input: './resource/js/main.js',
            },
            outDir: './public/dist',
            modulePreload: {
                polyfill: true,
            },
        },
        publicDir: false,
        css: {
            preprocessorOptions: {
                scss: {
                    silenceDeprecations: [
                        'import',
                        'mixed-decls',
                        'color-functions',
                        'global-builtin',
                        'if-function',
                    ],
                },
            },
        },
        plugins: [
            watchResourceDir(),
        ]
    }

    if (process.env.IS_DOCKER === '1') {
        let nginxAddr = await dns.lookup('nginx').then((result) => result.address)
        config.server = {
            cors: {
                // This needs to be equal to the url (origin) you see in the address bar
                origin: `http://${nginxAddr}`,
            },
            origin: `http://${nginxAddr}`,
        }
    }

    return config
})

function watchResourceDir() {
    return {
        name: 'vite-plugin-sturdy-framework',
        handleHotUpdate({ file, server }) {
            let pattern = `^${RegExp.escape(__dirname)}\\/target\\/debug\\/[^/]+\\.d$`
            let regex = new RegExp(pattern)
            if (file.startsWith(`${__dirname}/resource`) || regex.test(file)) {
                server.ws.send({ type: 'full-reload' })
            }

            return []
        },
    }
}
