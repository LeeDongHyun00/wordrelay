import {defineConfig} from '@playwright/test';
export default defineConfig({testDir:'tests/browser',timeout:30000,workers:1,use:{baseURL:process.env.BASE_URL||'http://127.0.0.1:3000',channel:'chrome',headless:true,trace:'retain-on-failure'},reporter:'list'});
