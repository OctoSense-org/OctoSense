import { AbilityConstant, UIAbility, Want } from '@kit.AbilityKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import { window } from '@kit.ArkUI';
import { BusinessError } from '@kit.BasicServicesKit';
import makepad from 'libmakepad.so'
import {ArkGlue} from '../makepad/makepad'

export default class EntryAbility extends UIAbility {
  private studioReview: boolean = false;

  onCreate(want: Want, launchParam: AbilityConstant.LaunchParam): void {
    let x = ArkGlue.initInstance(this.context);
    const parameters: Record<string, string> = {};
    for (const key of Object.keys(want.parameters ?? {})) {
      const value = want.parameters?.[key];
      if (key.startsWith('makepad.') && typeof value === 'string') {
        parameters[key] = value;
      }
    }
    x.launchParameters = JSON.stringify(parameters);
    this.studioReview = (parameters['makepad.STUDIO_BUILD'] ?? '').length > 0;
    makepad.onCreate(x);
    hilog.info(0x0000, 'testTag', '%{public}s', 'Ability onCreate');
  }

  onDestroy(): void {
    ArkGlue.getInstance().setLocationForeground(false);
    hilog.info(0x0000, 'testTag', '%{public}s', 'Ability onDestroy');
  }

  onWindowStageCreate(windowStage: window.WindowStage): void {
    // Main window is created, set main page for this ability
    hilog.info(0x0000, 'testTag', '%{public}s', 'Ability onWindowStageCreate');

    // Studio input reaches the native app rather than the OS input dispatcher,
    // so it does not reset the device's screen timeout during a review session.
    if (this.studioReview) {
      windowStage.getMainWindow().then((mainWindow) => {
        mainWindow.setWindowKeepScreenOn(true, (err) => {
          if (err.code) {
            hilog.warn(0x0000, 'testTag', 'Studio keep-screen-on failed: %{public}d', err.code);
          }
        });
      });
    }

    windowStage.loadContent('pages/Index', (err) => {
      if (err.code) {
        hilog.error(0x0000, 'testTag', 'Failed to load the content. Cause: %{public}s', JSON.stringify(err) ?? '');
        return;
      }
      hilog.info(0x0000, 'testTag', 'Succeeded in loading the content.');
    });

    // HarmonyOS lays the XComponent between its native system bars. Its
    // surface dimensions and local touch coordinates already exclude them.
    windowStage.getMainWindow().then((mainWindow) => {
      mainWindow.setWindowLayoutFullScreen(false).catch((e: BusinessError) => {
        hilog.warn(0x0000, 'testTag', 'native content layout failed: %{public}d', e.code);
      });
      mainWindow.setSpecificSystemBarEnabled('status', true).catch((e: BusinessError) => {
        hilog.warn(0x0000, 'testTag', 'show status bar failed: %{public}d', e.code);
      });
      mainWindow.setSpecificSystemBarEnabled('navigationIndicator', true).catch((e: BusinessError) => {
        hilog.warn(0x0000, 'testTag', 'show navigation indicator failed: %{public}d', e.code);
      });
      mainWindow.setWindowSystemBarProperties({
        statusBarColor: '#F9F7FC',
        navigationBarColor: '#F9F7FC',
        statusBarContentColor: '#261E34',
        navigationBarContentColor: '#261E34'
      }).catch((e: BusinessError) => {
        hilog.warn(0x0000, 'testTag', 'system bar properties failed: %{public}d', e.code);
      });
      // The surface is already inset by ArkUI; applying the window's bar
      // sizes to it a second time would shrink/misalign the app navigation.
      ArkGlue.getInstance().setAvoidArea(0, 0, 0, 0);
    }).catch((e: BusinessError) => {
      hilog.warn(0x0000, 'testTag', 'main window unavailable: %{public}d', e.code);
    });
  }

  onWindowStageDestroy(): void {
    // Main window is destroyed, release UI related resources
    hilog.info(0x0000, 'testTag', '%{public}s', 'Ability onWindowStageDestroy');
  }

  onForeground(): void {
    ArkGlue.getInstance().setLocationForeground(true);
    // Ability has brought to foreground
    hilog.info(0x0000, 'testTag', '%{public}s', 'Ability onForeground');
  }

  onBackground(): void {
    ArkGlue.getInstance().setLocationForeground(false);
    // Ability has back to background
    hilog.info(0x0000, 'testTag', '%{public}s', 'Ability onBackground');
  }
};
