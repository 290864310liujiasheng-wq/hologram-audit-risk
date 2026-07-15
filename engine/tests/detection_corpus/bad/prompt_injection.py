import openai

def chat(user_message):
    # AI-001: 用户输入直接注入 prompt
    response = openai.chat.completions.create(
        model="gpt-4",
        messages=[{"role": "user", "content": user_message}]
    )
    return response

def build_prompt(user_input):
    prompt = "You are helpful. Answer: " + user_input
    return prompt
