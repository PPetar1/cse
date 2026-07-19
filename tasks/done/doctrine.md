# Title
Doctrine

Status: done

## Goal

Implement doctrine faction wide modifier to the game and its subsystems

## Mechanics

Doctrine will exist as a faction scoped modifier settable in the scenario file. This will represent the overall quality of military tactics used in the faction army. It is primarly influenced by leaders which have their own personal doctrine rating that drifts over time towards the faction doctrine value. Both the leaders personal and the faction wide doctirne rating will be expressed as a number from 1 to 100. Leaders can also improve/worsten their own individual doctrine rating when participating in battles. Leaders will at the end of each battle gain (LAV - DOC/10) * FBO * LOS peronal doctrine points where LAV is the average of all of the leaders ratings except for political and air (there will also be an air doctrine in the future, but that is not in scope for this task), DOC is the leaders current personal doctrine value, FBO is the final battle odds for the leaders side, capped at 2 and at 0.5 (so if a leader wins with odds 3:1 FBO will be 2, if he loses with odds 4:1 for the enemy FBO will be 0.5), and LOS ranges from 0 to 1 based on the total battle losses (formula can for start be MAN/10000 where MAN is the amount of men losess in the battle (destroyed+damaged, retreat losses or surrendurs should not count)). Leader cannot by wining or losing points go over/under LAV * 10 personal doctrine points (if gain/loss would push him over that threshold set his DOC = LAV * 10). Leaders will also at the end of each turn drift towards the faction doctrine value using a formula ((FDO - DOC) / 10) * ((15 - INI) / 15) if the leader is losing doctrine or ((FDO - DOC) / 10) * ((15 - POL) / 15) if he is gaining doctrine in this way where FDO is faction doctrine value, DOC is the leaders doctrine rating, INI is the leaders initiative and POL is the leaders political rating. At the end of the turn, before this drift, each leader with doctrine higher than the faction doctrine will add ((DOC - FDO) / 100) * (1 / (11 - INI)) to the faction doctrine value and each leader with doctrine rating lower than FDO will subtract ((DOC - FDO) / 100) * (1 / (11 - POL)) from the faction doctrine. These additions and subtractions should happen after all have been calculated, so the leader going first doesn't already change the FDO and influence other leaders contributions by doing this.Also have faction doctrine value influence the combat performance of units, for now just mimic what experience does and do the same for doctrine. In the future leaders personal doctrine will also influence his battle rolls but that will be implemented later.
 
## Acceptance criteria

Doctrine exists as a faction scoped modifier. Leaders all have their doctrine rating which they gain or lose based on the outcome of the battles they are participating in and drift over time towards the faction doctrine value. All leaders influence the faction doctrine at the end of the turn. Units use faction doctrine value in battle in the same way experience is used.
