import os

from telegram import Update
from telegram.constants import ChatMemberStatus, ParseMode
from telegram.ext import Application, CommandHandler, ContextTypes, filters


RESIDENT_CHAT_ID = -1002614784999
XECUT_CHAT_ID = -1002089160630
XECUT_CHANNEL_TAG = "xecut_live"

async def post_handler(update: Update, context: ContextTypes.DEFAULT_TYPE) -> None:
    msg = update.effective_message

    if msg.chat_id != XECUT_CHAT_ID:
        return

    try:
        member = await context.bot.get_chat_member(RESIDENT_CHAT_ID, msg.from_user.id)

        if member.status not in (ChatMemberStatus.MEMBER, ChatMemberStatus.ADMINISTRATOR, ChatMemberStatus.OWNER):
            await msg.reply_text("Для постинга в Xecut Live нужно быть резидентом хакспейса")
            return

        if msg.reply_to_message:
            await context.bot.send_message(chat_id="@" + XECUT_CHANNEL_TAG,text=f"https://t.me/xecut_chat/{msg.reply_to_message.message_id}",disable_web_page_preview=True)

            posted = await context.bot.forward_message(
                chat_id="@" + XECUT_CHANNEL_TAG,
                from_chat_id=msg.chat_id,
                message_id=msg.reply_to_message.message_id,
            )
            
            await msg.reply_text(f'Запостил в <a href="https://t.me/{XECUT_CHANNEL_TAG}/{posted.message_id}">Xecut Live</a>', parse_mode=ParseMode.HTML, disable_web_page_preview=True)
        else:
            await msg.reply_text(f"Отправь эту команду реплаем на сообщение")

    except Exception as exc:
        print(exc)
        await msg.reply_text("Произошла ошибка, попробуйте позже")


token = os.getenv("XECUT_TG_API_KEY")
if not token:
    raise RuntimeError("Set XECUT_TG_API_KEY env variable with the bot token")

app = Application.builder().token(token).build()
app.add_handler(CommandHandler("post", post_handler))
app.run_polling()
