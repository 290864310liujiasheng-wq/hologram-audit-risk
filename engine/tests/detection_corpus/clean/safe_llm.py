import openai

SYSTEM_PROMPT = "You are a helpful assistant."

def chat(user_message: str):
    # 安全：系统提示固定，用户消息单独传入不拼接
    sanitized = user_message[:500].replace("<", "").replace(">", "")
    response = openai.chat.completions.create(
        model="gpt-4",
        messages=[
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": sanitized}
        ]
    )
    return response
