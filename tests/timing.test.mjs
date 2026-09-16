import {test} from 'node:test';
import assert from 'node:assert/strict';
import {displayTime,timerStage} from '../client/src/timing.ts';
test('clock is continuous at both slowdown boundaries and reaches zero only at deadline',()=>{
 for(const [real,display] of [[10000,6000],[6001,2001],[6000,2000],[5000,1500],[4001,1000.5],[4000,1000],[2000,500],[1,.25],[0,0],[-1,0]])assert.equal(displayTime(real),display);
 assert.equal(timerStage(2001),'normal');assert.equal(timerStage(2000),'slow');assert.equal(timerStage(1000),'critical');
 assert.equal(displayTime(5500)-displayTime(4500),500);
 assert.equal(displayTime(3000)-displayTime(2000),250);
});
