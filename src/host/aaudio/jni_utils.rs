extern crate jni;

use std::sync::Arc;

use self::jni::Executor;
use self::jni::{errors::Result as JResult, objects::JObject, JNIEnv, JavaVM};

pub fn with_attached<F, R>(closure: F) -> JResult<R>
where
    F: FnOnce(&JNIEnv, JObject) -> JResult<R>,
{
    let activity = ndk_glue::native_activity();
    let vm = Arc::new(unsafe { JavaVM::from_raw(activity.vm())? });
    let activity = activity.activity();
    Executor::new(vm).with_attached(|env| closure(env, activity.into()))
}

pub fn call_method_no_args_ret_int_array<'a>(
    env: &JNIEnv<'a>,
    subject: JObject,
    method: &str,
) -> JResult<Vec<i32>> {
    let array = env.auto_local(env.call_method(subject, method, "()[I", &[])?.l()?);

    let raw_array = array.as_obj().into_inner();

    let length = env.get_array_length(raw_array)?;
    let mut values = Vec::with_capacity(length as usize);

    env.get_int_array_region(raw_array, 0, values.as_mut())?;

    Ok(values)
}

pub fn call_method_no_args_ret_int<'a>(
    env: &JNIEnv<'a>,
    subject: JObject,
    method: &str,
) -> JResult<i32> {
    env.call_method(subject, method, "()I", &[])?.i()
}

pub fn call_method_no_args_ret_bool<'a>(
    env: &JNIEnv<'a>,
    subject: JObject,
    method: &str,
) -> JResult<bool> {
    env.call_method(subject, method, "()Z", &[])?.z()
}

pub fn call_method_no_args_ret_string<'a>(
    env: &JNIEnv<'a>,
    subject: JObject,
    method: &str,
) -> JResult<String> {
    env.get_string(
        env.call_method(subject, method, "()Ljava/lang/String;", &[])?
            .l()?
            .into(),
    )
    .map(String::from)
}

pub fn call_method_no_args_ret_char_sequence<'a>(
    env: &JNIEnv<'a>,
    subject: JObject,
    method: &str,
) -> JResult<String> {
    env.get_string(
        env.call_method(
            env.call_method(subject, method, "()Ljava/lang/CharSequence;", &[])?
                .l()?,
            "toString",
            "()Ljava/lang/String;",
            &[],
        )?
        .l()?
        .into(),
    )
    .map(String::from)
}

pub fn call_method_string_arg_ret_object<'a>(
    env: &JNIEnv<'a>,
    subject: JObject<'a>,
    method: &str,
    arg: &str,
) -> JResult<JObject<'a>> {
    env.call_method(
        subject,
        method,
        "(Ljava/lang/String;)Ljava/lang/Object;",
        &[JObject::from(env.new_string(arg)?).into()],
    )?
    .l()
}

pub fn get_system_service<'a>(
    env: &JNIEnv<'a>,
    subject: JObject<'a>,
    name: &str,
) -> JResult<JObject<'a>> {
    call_method_string_arg_ret_object(env, subject, "getSystemService", name)
}
