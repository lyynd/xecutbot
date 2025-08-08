FROM python:3.12.4-bookworm
WORKDIR /usr/src/app

COPY ./requirements.txt ./requirements.txt
RUN pip install --no-cache-dir -r requirements.txt

COPY ./main.py ./
CMD [ "python", "./main.py" ]