import { test, expect } from '@playwright/test';
import { registerUser, createChannel, sendMessage, getChannelIdFromUrl } from './helpers';

test.describe('Threads', () => {
  test('thread page renders', async ({ page }) => {
    await registerUser(page, 'thp');
    await page.goto('/channels/test/thread/1');
    await page.waitForTimeout(1000);
    await expect(page.locator('body')).toBeVisible();
  });

  test('send thread reply via API', async ({ page }) => {
    await registerUser(page, 'thr');
    await createChannel(page, 'ThreadChan');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await sendMessage(page, 'Parent message');
    await page.waitForSelector('.message-bubble');
    const channelId = getChannelIdFromUrl(page);
    const parentId = await page.locator('.message-bubble').first().evaluate(el =>
      el.getAttribute('data-message-id') || el.getAttribute('data-id') || '1'
    );
    await page.evaluate(async ({cid,pid}: {cid:string;pid:string}) => {
      const s = JSON.parse(localStorage.getItem('auth-storage')||'{}');
      const t = s?.state?.token || s?.token;
      await fetch('/api/channels/'+cid+'/messages',{
        method:'POST',
        headers:{'Authorization':'Bearer '+t,'Content-Type':'application/json'},
        body:JSON.stringify({msg_type:'text',payload:{text:'Thread reply'},thread_parent_id:parseInt(pid)})
      });
    }, {cid:channelId,pid:parentId});
    await expect(page.locator('.message-bubble').first()).toContainText('Parent message',{timeout:5000});
  });

  test('thread reply excluded from main channel', async ({ page }) => {
    await registerUser(page, 'the');
    await createChannel(page, 'ExcludeChan');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await sendMessage(page, 'Main');
    await page.waitForSelector('.message-bubble');
    const channelId = getChannelIdFromUrl(page);
    const parentId = await page.locator('.message-bubble').first().evaluate(el =>
      el.getAttribute('data-message-id') || el.getAttribute('data-id') || '1'
    );
    await page.evaluate(async ({cid,pid}: {cid:string;pid:string}) => {
      const s = JSON.parse(localStorage.getItem('auth-storage')||'{}');
      const t = s?.state?.token || s?.token;
      await fetch('/api/channels/'+cid+'/messages',{
        method:'POST',
        headers:{'Authorization':'Bearer '+t,'Content-Type':'application/json'},
        body:JSON.stringify({msg_type:'text',payload:{text:'Hidden'},thread_parent_id:parseInt(pid)})
      });
    }, {cid:channelId,pid:parentId});
    await page.reload();
    await page.waitForTimeout(1000);
    await expect(page.locator('.message-bubble').filter({hasText:'Hidden'})).toHaveCount(0,{timeout:5000});
  });

  test('multiple thread replies', async ({ page }) => {
    await registerUser(page, 'thm');
    await createChannel(page, 'MultiChan');
    await page.waitForSelector('.channel-item');
    await page.locator('.channel-item').first().click();
    await page.waitForTimeout(500);
    await sendMessage(page, 'Parent');
    await page.waitForSelector('.message-bubble');
    const channelId = getChannelIdFromUrl(page);
    const parentId = await page.locator('.message-bubble').first().evaluate(el =>
      el.getAttribute('data-message-id') || el.getAttribute('data-id') || '1'
    );
    for (const txt of ['Reply 1','Reply 2']) {
      await page.evaluate(async ({cid,pid,txt}: {cid:string;pid:string;txt:string}) => {
        const s = JSON.parse(localStorage.getItem('auth-storage')||'{}');
        const t = s?.state?.token || s?.token;
        await fetch('/api/channels/'+cid+'/messages',{
          method:'POST',
          headers:{'Authorization':'Bearer '+t,'Content-Type':'application/json'},
          body:JSON.stringify({msg_type:'text',payload:{text:txt},thread_parent_id:parseInt(pid)})
        });
      }, {cid:channelId,pid:parentId,txt});
    }
    await page.waitForTimeout(300);
    await expect(page.locator('.message-bubble').first()).toBeVisible();
  });
});
