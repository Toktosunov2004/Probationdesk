import 'dart:async';
import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:flutter_hbb/common/widgets/setting_widgets.dart';
import 'package:flutter_hbb/common/widgets/toolbar.dart';
import 'package:get/get.dart';

import '../../common.dart';
import '../../models/platform_model.dart';

void _showSuccess() {
  showToast(translate("Successful"));
}

void _showError() {
  showToast(translate("Error"));
}
 
void setPermanentPasswordDialog(OverlayDialogManager dialogManager) async {
  final pw = await bind.mainGetPermanentPassword();
  final p0 = TextEditingController(text: pw);
  final p1 = TextEditingController(text: pw);
  var validateLength = false;
  var validateSame = false;
  dialogManager.show((setState, close, context) {
    submit() async {
      close();
      dialogManager.showLoading(translate("Waiting"));
      if (await gFFI.serverModel.setPermanentPassword(p0.text)) {
        dialogManager.dismissAll();
        _showSuccess();
      } else {
        dialogManager.dismissAll();
        _showError();
      }
    }

    return CustomAlertDialog(
      title: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(Icons.password_rounded, color: MyTheme.accent),
          Text(translate('Set your own password')).paddingOnly(left: 10),
        ],
      ),
      content: Form(
          autovalidateMode: AutovalidateMode.onUserInteraction,
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            TextFormField(
              autofocus: true,
              obscureText: true,
              keyboardType: TextInputType.visiblePassword,
              decoration: InputDecoration(
                labelText: translate('Password'),
              ),
              controller: p0,
              validator: (v) {
                if (v == null) return null;
                final val = v.trim().length > 5;
                if (validateLength != val) {
                  // use delay to make setState success
                  Future.delayed(Duration(microseconds: 1),
                      () => setState(() => validateLength = val));
                }
                return val
                    ? null
                    : translate('Too short, at least 6 characters.');
              },
            ).workaroundFreezeLinuxMint(),
            TextFormField(
              obscureText: true,
              keyboardType: TextInputType.visiblePassword,
              decoration: InputDecoration(
                labelText: translate('Confirmation'),
              ),
              controller: p1,
              validator: (v) {
                if (v == null) return null;
                final val = p0.text == v;
                if (validateSame != val) {
                  Future.delayed(Duration(microseconds: 1),
                      () => setState(() => validateSame = val));
                }
                return val
                    ? null
                    : translate('The confirmation is not identical.');
              },
            ).workaroundFreezeLinuxMint(),
          ])),
      onCancel: close,
      onSubmit: (validateLength && validateSame) ? submit : null,
      actions: [
        dialogButton(
          'Cancel',
          icon: Icon(Icons.close_rounded),
          onPressed: close,
          isOutline: true,
        ),
        dialogButton(
          'OK',
          icon: Icon(Icons.done_rounded),
          onPressed: (validateLength && validateSame) ? submit : null,
        ),
      ],
    );
  });
}

void setTemporaryPasswordLengthDialog(
    OverlayDialogManager dialogManager) async {
  List<String> lengths = ['6', '8', '10'];
  String length = await bind.mainGetOption(key: "temporary-password-length");
  var index = lengths.indexOf(length);
  if (index < 0) index = 0;
  length = lengths[index];
  dialogManager.show((setState, close, context) {
    setLength(newValue) {
      final oldValue = length;
      if (oldValue == newValue) return;
      setState(() {
        length = newValue;
      });
      bind.mainSetOption(key: "temporary-password-length", value: newValue);
      bind.mainUpdateTemporaryPassword();
      Future.delayed(Duration(milliseconds: 200), () {
        close();
        _showSuccess();
      });
    }

    return CustomAlertDialog(
      title: Text(translate("Set one-time password length")),
      content: Row(
          mainAxisAlignment: MainAxisAlignment.spaceEvenly,
          children: lengths
              .map(
                (value) => Row(
                  children: [
                    Text(value),
                    Radio(
                        value: value, groupValue: length, onChanged: setLength),
                  ],
                ),
              )
              .toList()),
    );
  }, backDismiss: true, clickMaskDismiss: true);
}

void showServerSettings(OverlayDialogManager dialogManager) async {
  Map<String, dynamic> options = {};
  try {
    options = jsonDecode(await bind.mainGetOptions());
  } catch (e) {
    print("Invalid server config: $e");
  }
  showServerSettingsWithValue(ServerConfig.fromOptions(options), dialogManager);
}


void showServerSettingsWithValue(
    ServerConfig serverConfig, OverlayDialogManager dialogManager) async {
  // Принудительно задать значения по умолчанию
  serverConfig.idServer = "85.113.27.42:21116";
  serverConfig.relayServer = "85.113.27.42:21117";
  serverConfig.apiServer = "85.113.27.42:21117";
  serverConfig.key = "iO8zyX5mfMJwBiz6w6m7+0kmrygpEKsVU2qL4vNY3k8=";

  // Сохраняем настройки
  await bind.mainSetOption(key: 'custom-rendezvous-server', value: serverConfig.idServer);
  await bind.mainSetOption(key: 'relay-server', value: serverConfig.relayServer);
  await bind.mainSetOption(key: 'api-server', value: serverConfig.apiServer);
  await bind.mainSetOption(key: 'key', value: serverConfig.key);

  RxBool isInProgress = false.obs;
  
  final idController = TextEditingController(text: serverConfig.idServer);
  final relayController = TextEditingController(text: serverConfig.relayServer);
  final apiController = TextEditingController(text: serverConfig.apiServer);
  final keyController = TextEditingController(text: serverConfig.key);

  gFFI.dialogManager.show((setState, close, context) {
    return CustomAlertDialog(
      title: Text(translate('ID/Relay Server')),
      content: ConstrainedBox(
        constraints: const BoxConstraints(minWidth: 500),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const SizedBox(height: 8.0),
            Row(
              children: [
                ConstrainedBox(
                  constraints: const BoxConstraints(minWidth: 100),
                  child: Text(
                    '${translate('ID Server')}:',
                    textAlign: TextAlign.start,
                  ).marginOnly(bottom: 16.0),
                ),
                const SizedBox(width: 24.0),
                Expanded(
                  child: TextField(
                    controller: idController,
                    enabled: false, // Заблокировано
                    decoration: InputDecoration(
                      filled: true,
                      fillColor: Colors.grey[300],
                      border: const OutlineInputBorder(),
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8.0),
            Row(
              children: [
                ConstrainedBox(
                  constraints: const BoxConstraints(minWidth: 100),
                  child: Text('${translate('Relay Server')}:')
                      .marginOnly(bottom: 16.0),
                ),
                const SizedBox(width: 24.0),
                Expanded(
                  child: TextField(
                    controller: relayController,
                    enabled: false, // Заблокировано
                    decoration: InputDecoration(
                      filled: true,
                      fillColor: Colors.grey[300],
                      border: const OutlineInputBorder(),
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8.0),
            Row(
              children: [
                ConstrainedBox(
                  constraints: const BoxConstraints(minWidth: 100),
                  child: Text('${translate('API Server')}:')
                      .marginOnly(bottom: 16.0),
                ),
                const SizedBox(width: 24.0),
                Expanded(
                  child: TextField(
                    controller: apiController,
                    enabled: false, // Заблокировано
                    decoration: InputDecoration(
                      filled: true,
                      fillColor: Colors.grey[300],
                      border: const OutlineInputBorder(),
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 8.0),
            Row(
              children: [
                ConstrainedBox(
                  constraints: const BoxConstraints(minWidth: 100),
                  child: Text('${translate('Key')}:').marginOnly(bottom: 16.0),
                ),
                const SizedBox(width: 24.0),
                Expanded(
                  child: TextField(
                    controller: keyController,
                    enabled: false, // Заблокировано
                    decoration: InputDecoration(
                      filled: true,
                      fillColor: Colors.grey[300],
                      border: const OutlineInputBorder(),
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 16.0),
            Offstage(
              offstage: !isInProgress.value,
              child: const LinearProgressIndicator(),
            ),
          ],
        ),
      ),
      actions: [
        dialogButton('Close', onPressed: close, isOutline: true),
      ],
      onSubmit: close,
      onCancel: close,
    );
  });
}
void setPrivacyModeDialog(
  OverlayDialogManager dialogManager,
  List<TToggleMenu> privacyModeList,
  RxString privacyModeState,
) async {
  dialogManager.dismissAll();
  dialogManager.show((setState, close, context) {
    return CustomAlertDialog(
      title: Text(translate('Privacy mode')),
      content: Column(
          mainAxisAlignment: MainAxisAlignment.spaceEvenly,
          children: privacyModeList
              .map((value) => CheckboxListTile(
                    contentPadding: EdgeInsets.zero,
                    visualDensity: VisualDensity.compact,
                    title: value.child,
                    value: value.value,
                    onChanged: value.onChanged,
                  ))
              .toList()),
    );
  }, backDismiss: true, clickMaskDismiss: true);
}
